//! panel-ddp: draws dashboard frames and streams them to a WLED matrix.
//!
//!     panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
//!     panel-ddp preview [--gaps FILE] [--out FILE]
//!
//! HOST defaults to 4.3.2.1 (WLED-AP), PORT to 4048.

mod canvas;
mod ddp;
mod mask;
mod testframe;

use anyhow::{bail, Context, Result};
use canvas::Canvas;
use ddp::DdpSender;
use embedded_graphics::pixelcolor::Rgb888;
use mask::Mask;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const USAGE: &str = "usage:
  panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
  panel-ddp preview [--gaps FILE] [--out FILE]";

/// The gap file, relative to this crate, when run from anywhere in the repo.
const DEFAULT_GAPS: &str = "2d-gaps.json";

struct Stream {
    target: String,
    fps: u32,
    frames: Option<u32>,
}

fn parse_stream(args: &[String]) -> Result<Stream> {
    let mut s = Stream { target: "4.3.2.1:4048".into(), fps: 10, frames: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fps" => {
                s.fps = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&v| v > 0)
                    .context("--fps needs a positive number")?
            }
            "--frames" => {
                s.frames = Some(
                    it.next().and_then(|v| v.parse().ok()).context("--frames needs a number")?,
                )
            }
            o if o.starts_with('-') => bail!("unknown option {o}"),
            host => s.target = ddp::target_with_default_port(host),
        }
    }
    Ok(s)
}

/// Runs `draw` for each frame and streams the result, paced from the start
/// time so drawing cost does not drift the rate.
fn stream(s: &Stream, mut draw: impl FnMut(&mut Canvas, u32)) -> Result<()> {
    let mut sender = DdpSender::new(&s.target)
        .with_context(|| format!("opening UDP socket to {}", s.target))?;
    eprintln!(
        "panel-ddp: streaming to {} at {} fps{}",
        s.target,
        s.fps,
        match s.frames {
            Some(n) => format!(", {n} frames"),
            None => ", Ctrl-C to stop".into(),
        }
    );
    let period = Duration::from_secs_f64(1.0 / s.fps as f64);
    let mut canvas = Canvas::new();
    let start = Instant::now();
    let mut frame = 0u32;
    while s.frames.map_or(true, |n| frame < n) {
        draw(&mut canvas, frame);
        sender.send_frame(&canvas.px).context("sending frame")?;
        frame += 1;
        let due = start + period * frame;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    Ok(())
}

fn cmd_test(args: &[String]) -> Result<()> {
    let s = parse_stream(args)?;
    stream(&s, testframe::draw)
}

fn cmd_preview(args: &[String]) -> Result<()> {
    let mut gaps = PathBuf::from(DEFAULT_GAPS);
    let mut out = PathBuf::from("preview.png");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--gaps" => gaps = it.next().context("--gaps needs a file")?.into(),
            "--out" => out = it.next().context("--out needs a file")?.into(),
            o => bail!("unknown option {o}"),
        }
    }
    let mask = if gaps.exists() {
        Mask::load(&gaps)?
    } else {
        eprintln!("panel-ddp: no gap file at {}; the preview shows the whole panel", gaps.display());
        Mask::none()
    };
    let mut canvas = Canvas::new();
    mask::draw_outline(&mut canvas, &mask, Rgb888::new(20, 20, 20));
    testframe::draw(&mut canvas, 0);
    mask.preview_png(&canvas, 4, &out)?;
    eprintln!("panel-ddp: wrote {}", out.display());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let result = match cmd {
        "test" => cmd_test(rest),
        "preview" => cmd_preview(rest),
        "-h" | "--help" => {
            eprintln!("{USAGE}");
            return;
        }
        other => Err(anyhow::anyhow!("unknown command {other}")),
    };
    if let Err(e) = result {
        eprintln!("panel-ddp: {e:#}");
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
}
