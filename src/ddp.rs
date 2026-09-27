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
/// Bytes per pixel; packets split between pixels.
const PIXEL: usize = 3;

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
        for (offset, len) in chunks(rgb.len()) {
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
        }
        // Sequence numbers run 1..=15; 0 means "not used".
        self.seq = if self.seq >= 15 { 1 } else { self.seq + 1 };
        Ok(())
    }
}

/// Where each packet's data starts in a frame of `len` bytes, and how long
/// it is. As few packets as DDP's limit allows, sharing the frame evenly and
/// splitting between pixels, rather than full packets and a short last one:
/// a 64x64 frame is nine packets of 1368 bytes but the last, 1406-byte
/// datagrams that cross links with an MTU down to that (a VPN tunnel's is
/// often 1420) without being fragmented.
fn chunks(len: usize) -> Vec<(usize, usize)> {
    if len == 0 {
        return Vec::new();
    }
    let packets = len.div_ceil(MAX_DATA);
    let each = len.div_ceil(packets).div_ceil(PIXEL) * PIXEL;
    (0..len).step_by(each).map(|offset| (offset, each.min(len - offset))).collect()
}

/// Resolves "host" or "host:port" into a target string, defaulting the port.
pub fn target_with_default_port(host: &str) -> String {
    if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:4048")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_split_evenly_between_pixels() {
        let frame = 64 * 64 * 3;
        let c = chunks(frame);
        assert_eq!(c.len(), 9, "as many packets as full ones would take");
        assert!(c.iter().all(|&(o, l)| o % PIXEL == 0 && l % PIXEL == 0 && l <= MAX_DATA));
        assert_eq!(c[0], (0, 1368));
        assert_eq!(c[8], (8 * 1368, 1344));
        assert_eq!(c.iter().map(|&(_, l)| l).sum::<usize>(), frame);
        assert!(c.windows(2).all(|w| w[0].0 + w[0].1 == w[1].0), "contiguous");
        assert_eq!(HEADER + 1368 + 8 + 20, 1406, "the largest IP datagram");
        for len in [3, 1440, 1443, 30000] {
            let c = chunks(len);
            assert_eq!(c.len(), len.div_ceil(MAX_DATA));
            assert_eq!(c.iter().map(|&(_, l)| l).sum::<usize>(), len);
        }
        assert!(chunks(0).is_empty());
    }
}
