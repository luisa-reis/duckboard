//! Streams a test frame to the panel over DDP (Distributed Display Protocol,
//! UDP port 4048), which WLED listens on out of the box. Each frame is drawn
//! into a 64x64 RGB canvas with embedded-graphics and sent as raw RGB bytes;
//! WLED shows it as realtime input and falls back to its presets a couple of
//! seconds after the stream stops.
//!
//!     panel-ddp [HOST[:PORT]] [--fps N] [--frames N]
//!
//! HOST defaults to 4.3.2.1 (WLED-AP), PORT to 4048.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use std::convert::Infallible;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

/// DDP header, 10 bytes, all multi-byte fields big-endian.
const DDP_HEADER: usize = 10;
/// Bits of byte 0: protocol version 1, and "push" (show the frame now).
const DDP_VER1: u8 = 0x40;
const DDP_PUSH: u8 = 0x01;
/// Byte 2, data type: RGB, 8 bits per channel. WLED keys its bytes-per-pixel
/// on this and would read RGBW for 0x1B.
const DDP_TYPE_RGB8: u8 = 0x0B;
/// Byte 3, destination: 1 is the default output.
const DDP_DEST_DEFAULT: u8 = 0x01;
/// Longest data payload a packet may carry (480 pixels): DDP's limit, and it
/// keeps each datagram under a 1500-byte MTU with the header.
const DDP_MAX_DATA: usize = 1440;

/// A plain row-major RGB canvas that embedded-graphics can draw into. Its
/// bytes are already the wire format DDP wants, so a frame is sent as-is.
struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        Self { px: vec![0; (WIDTH * HEIGHT * 3) as usize] }
    }
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Canvas {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x < 0 || p.y < 0 || p.x >= WIDTH as i32 || p.y >= HEIGHT as i32 {
                continue;
            }
            let i = ((p.y as u32 * WIDTH + p.x as u32) * 3) as usize;
            self.px[i] = c.r();
            self.px[i + 1] = c.g();
            self.px[i + 2] = c.b();
        }
        Ok(())
    }
}

/// Sends one RGB frame as a run of DDP packets. Only the last packet carries
/// the push flag, so WLED shows the frame once all of it has arrived.
struct DdpSender {
    sock: UdpSocket,
    target: SocketAddr,
    seq: u8,
}

impl DdpSender {
    /// The socket stays unconnected: a connected UDP socket reports ICMP
    /// unreachables as send errors, which would end the stream whenever the
    /// board reboots. Unconnected, the datagrams are simply lost until it is
    /// back, which is what a display feed wants.
    fn new(target: &str) -> std::io::Result<Self> {
        let target = target
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other(format!("{target} does not resolve")))?;
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self { sock, target, seq: 1 })
    }

    fn send_frame(&mut self, rgb: &[u8]) -> std::io::Result<()> {
        let mut packet = Vec::with_capacity(DDP_HEADER + DDP_MAX_DATA);
        let mut offset = 0usize;
        while offset < rgb.len() {
            let len = (rgb.len() - offset).min(DDP_MAX_DATA);
            let last = offset + len >= rgb.len();
            packet.clear();
            packet.push(DDP_VER1 | if last { DDP_PUSH } else { 0 });
            packet.push(self.seq);
            packet.push(DDP_TYPE_RGB8);
            packet.push(DDP_DEST_DEFAULT);
            packet.extend_from_slice(&(offset as u32).to_be_bytes());
            packet.extend_from_slice(&(len as u16).to_be_bytes());
            packet.extend_from_slice(&rgb[offset..offset + len]);
            self.sock.send_to(&packet, self.target)?;
            offset += len;
        }
        // Sequence numbers run 1..=15; 0 means "not used".
        self.seq = if self.seq >= 15 { 1 } else { self.seq + 1 };
        Ok(())
    }
}

/// The test frame: a border so the edges are visible, RGB and white bars to
/// check colour order and brightness, a frame counter and a bouncing dot so a
/// stuck stream is obvious.
fn draw_test_frame(c: &mut Canvas, frame: u32) {
    c.clear(Rgb888::BLACK).unwrap();

    let border = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(64, 64, 64), 1));
    border.draw(c).unwrap();

    // Colour bars across the top: R G B W, each 14 wide.
    let bars = [
        Rgb888::RED,
        Rgb888::GREEN,
        Rgb888::BLUE,
        Rgb888::WHITE,
    ];
    for (i, colour) in bars.iter().enumerate() {
        Rectangle::new(Point::new(4 + i as i32 * 14, 4), Size::new(14, 8))
            .into_styled(PrimitiveStyle::with_fill(*colour))
            .draw(c)
            .unwrap();
    }

    // A grey ramp under the bars: 56 steps, for gamma and bit depth.
    for x in 0..56i32 {
        let v = (x * 255 / 55) as u8;
        Line::new(Point::new(4 + x, 14), Point::new(4 + x, 17))
            .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(v, v, v), 1))
            .draw(c)
            .unwrap();
    }

    let big = MonoTextStyle::new(&FONT_6X10, Rgb888::new(255, 200, 0));
    Text::with_alignment("DDP", Point::new(32, 31), big, Alignment::Center)
        .draw(c)
        .unwrap();

    let small = MonoTextStyle::new(&FONT_4X6, Rgb888::new(0, 200, 255));
    Text::with_alignment(
        &format!("{:05}", frame),
        Point::new(32, 41),
        small,
        Alignment::Center,
    )
    .draw(c)
    .unwrap();

    // A dot bouncing along the bottom lane, one pixel per frame.
    let lane = (WIDTH - 12) as i32;
    let t = frame as i32 % (2 * lane);
    let x = if t < lane { t } else { 2 * lane - t };
    Circle::new(Point::new(4 + x, 50), 5)
        .into_styled(PrimitiveStyle::with_fill(Rgb888::MAGENTA))
        .draw(c)
        .unwrap();
}

struct Args {
    target: String,
    fps: u32,
    frames: Option<u32>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { target: "4.3.2.1:4048".into(), fps: 10, frames: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fps" => {
                args.fps = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&v| v > 0)
                    .ok_or("--fps needs a positive number")?
            }
            "--frames" => {
                args.frames = Some(
                    it.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--frames needs a number")?,
                )
            }
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            host => {
                args.target = if host.contains(':') { host.into() } else { format!("{host}:4048") }
            }
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("panel-ddp: {e}");
            }
            eprintln!("usage: panel-ddp [HOST[:PORT]] [--fps N] [--frames N]");
            std::process::exit(if e.is_empty() { 0 } else { 2 });
        }
    };

    let mut sender = match DdpSender::new(&args.target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("panel-ddp: cannot open UDP socket to {}: {e}", args.target);
            std::process::exit(1);
        }
    };
    eprintln!(
        "panel-ddp: streaming {WIDTH}x{HEIGHT} to {} at {} fps{}",
        args.target,
        args.fps,
        match args.frames {
            Some(n) => format!(", {n} frames"),
            None => ", Ctrl-C to stop".into(),
        }
    );

    let period = Duration::from_secs_f64(1.0 / args.fps as f64);
    let mut canvas = Canvas::new();
    let start = Instant::now();
    let mut frame = 0u32;
    loop {
        if args.frames.is_some_and(|n| frame >= n) {
            break;
        }
        draw_test_frame(&mut canvas, frame);
        if let Err(e) = sender.send_frame(&canvas.px) {
            eprintln!("panel-ddp: send failed: {e}");
            std::process::exit(1);
        }
        frame += 1;
        // Pace from the start time so drawing cost does not drift the rate.
        let due = start + period * frame;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}
