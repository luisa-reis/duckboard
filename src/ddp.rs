//! DDP (Distributed Display Protocol) sender. WLED listens on UDP 4048 out
//! of the box and shows what arrives as realtime input, falling back to its
//! presets a couple of seconds after the stream stops.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

/// Header is 10 bytes, all multi-byte fields big-endian.
const HEADER: usize = 10;
/// Byte 0: protocol version 1, and "push" (show the frame now).
const VER1: u8 = 0x40;
const PUSH: u8 = 0x01;
/// Byte 2, data type: RGB, 8 bits per channel. WLED keys its bytes-per-pixel
/// on bits 3..5 of this and would read RGBW for 0x1B.
const TYPE_RGB8: u8 = 0x0B;
/// Byte 3, destination: 1 is the default output.
const DEST_DEFAULT: u8 = 0x01;
/// Longest data payload a packet may carry (480 pixels): DDP's limit, and it
/// keeps each datagram under a 1500-byte MTU with the header.
const MAX_DATA: usize = 1440;

pub struct DdpSender {
    sock: UdpSocket,
    target: SocketAddr,
    seq: u8,
}

impl DdpSender {
    /// The socket stays unconnected: a connected UDP socket reports ICMP
    /// unreachables as send errors, which would end the stream whenever the
    /// board reboots. Unconnected, the datagrams are simply lost until it is
    /// back, which is what a display feed wants.
    pub fn new(target: &str) -> std::io::Result<Self> {
        let target = target
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other(format!("{target} does not resolve")))?;
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self { sock, target, seq: 1 })
    }

    /// Sends one RGB frame as a run of packets. Only the last carries the
    /// push flag, so WLED shows the frame once all of it has arrived.
    pub fn send_frame(&mut self, rgb: &[u8]) -> std::io::Result<()> {
        let mut packet = Vec::with_capacity(HEADER + MAX_DATA);
        let mut offset = 0usize;
        while offset < rgb.len() {
            let len = (rgb.len() - offset).min(MAX_DATA);
            let last = offset + len >= rgb.len();
            packet.clear();
            packet.push(VER1 | if last { PUSH } else { 0 });
            packet.push(self.seq);
            packet.push(TYPE_RGB8);
            packet.push(DEST_DEFAULT);
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

/// Resolves "host" or "host:port" into a target string, defaulting the port.
pub fn target_with_default_port(host: &str) -> String {
    if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:4048")
    }
}
