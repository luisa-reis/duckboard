# DDP, the Distributed Display Protocol

DDP is how `panel-ddp` gets pixels onto the board. It is a small, open UDP
protocol from 3waylabs for sending real-time data to LED displays: a
10-byte header, then raw pixel bytes. WLED receives it on UDP port 4048
out of the box, with nothing to install on the board.

This page covers the protocol itself, what WLED does with it, how
`src/ddp.rs` uses it, and what it cannot do. The spec is short and worth a
read: <http://www.3waylabs.com/ddp/> (plain HTTP only).

## The model

A display holds a frame buffer. A controller (here, `panel-ddp`) writes
blocks of bytes into it, each block saying where it goes (a byte offset)
and how long it is. Blocks may arrive in any order and the buffer is not
cleared between frames, so a controller may send only what changed. A
*push* tells the display to show what the buffer holds.

That is all DDP knows: a flat run of bytes. It has no idea of rows, of a
matrix and its size, of regions or of anything drawn in them. Everything visual
is decided by the sender before the bytes leave.

## Packet format

Every packet is a 10-byte header (14 with a timecode) and then the data.
Multi-byte fields are big-endian.

| Byte  | Field       | Meaning |
|-------|-------------|---------|
| 0     | flags       | `V V x T S R Q P`, below |
| 1     | sequence    | low 4 bits: 1–15, or 0 for unused; high 4 bits reserved |
| 2     | data type   | `C R T T T S S S`, below |
| 3     | ID          | destination (or source, in a reply), below |
| 4–7   | offset      | where the data goes in the buffer, in bytes |
| 8–9   | length      | bytes of data that follow |
| 10–13 | timecode    | only when the T flag is set |

**Flags (byte 0)**

| Bits | Name     | Meaning |
|------|----------|---------|
| 7–6  | version  | `01` for version 1, so every packet has `0x40` set |
| 5    | reserved | 0 |
| 4    | T        | a timecode follows the header |
| 3    | S        | take the data from the display's storage, not the packet |
| 2    | R        | reply, sent by a display answering a query |
| 1    | Q        | query: read `length` bytes at `offset` from the ID |
| 0    | P        | push: show the buffer now (or: last packet of a reply) |

**Data type (byte 2)**: `C` is 1 for customer-defined types, `R` is
reserved, `TTT` is the kind (0 undefined, 1 RGB, 2 HSL, 3 RGBW,
4 greyscale) and `SSS` the bits per channel (0 undefined, then 1, 4, 8, 16,
24, 32). RGB at 8 bits per channel is `00 001 011`, `0x0B`; RGBW is `0x1B`.
Older senders use `0x01`, and 0 means "not said".

**IDs (byte 3)**: 1 is the default output. 2–249 are free for a display to
define, for example as a sub-region of its output. 246 is JSON control,
250 JSON config, 251 JSON status, 254 DMX transit and 255 all devices.

**Sequence (byte 1)**: the spec says to increment it for each packet, and
a receiver may drop back-to-back duplicates, which lets a sender repeat
packets for redundancy.

**Size**: the length field allows 65,535 bytes, but the spec's own limit
is 1,440 bytes of data (480 RGB pixels), so that a packet with its headers
fits a 1,500-byte Ethernet MTU without fragmenting.

**Timecode**: the middle 32 bits of an NTP timestamp (16 bits of seconds,
16 of fraction). Sent with a push, it asks the display to show the frame at
that moment, which synchronises several displays whose clocks agree.

The spec also defines queries and replies, JSON discovery, config and
status, and a way to carry DMX. None of it concerns pixels on this panel,
and WLED implements none of it (below).

## What WLED does with it

From WLED's receiver (`handleDDPPacket` in `wled00/e131.cpp` on the `main`
branch, September 2026):

- **Types**: RGB or RGBW, chosen by the `TTT` bits alone. Anything else,
  including an undefined type, is read as 8-bit RGB. Other bit depths are
  not supported.
- **Offsets**: a packet's pixels start at `offset / 3` (or `/ 4` for
  RGBW), plus WLED's DMX start address setting divided the same way (the
  default, 1, adds nothing) and its *Realtime LED offset* (default 0).
- **Push**: pixels are written as each packet arrives but only shown on a
  push. Until a sender's first push, every packet is shown straight away,
  so senders that never push still work.
- **Sequence**: with *Skip out-of-sequence packets* on (off by default),
  packets from a frame older than the last one pushed are dropped. The
  check assumes at most a few packets per frame.
- **Ignored**: queries, replies, storage, the JSON IDs (246, 250, 251) and
  the timecode (the header is skipped over, but the time is not used).
  Every other ID writes the same output, so custom IDs cannot address
  sub-regions.
- **Realtime mode**: the first DDP packet puts WLED into realtime mode,
  and it stays there until no packet has arrived for the realtime timeout
  (2,500 ms by default), then returns to its presets. There is one
  realtime buffer: WLED notes the latest sender's address but does not
  lock others out.
- **Gamma and brightness**: realtime data skips WLED's gamma correction by
  default (*Disable realtime gamma correction*), which is why `panel-ddp`
  applies its own. *Force max brightness* and *Use main segment only* are
  further realtime settings.

## How panel-ddp uses it

`src/ddp.rs` sends every frame whole:

- The canvas is the panel's size, 64×64 unless the config says otherwise,
  in RGB (12,288 bytes at 64×64) and row-major order: pixel (x, y) is at
  byte `(y × width + x) × 3`. The board's 2D settings map that to the
  physical LEDs, so they must be set up for the same width and height.
- A frame goes out as nine packets, the fewest the 1,440-byte limit
  allows, sharing it evenly and split between pixels: eight of 1,368 bytes
  and one of 1,344, rather than eight full ones and a short last one. The
  largest datagram is then 1,406 bytes, which crosses a link with a smaller
  MTU than Ethernet's, such as a VPN tunnel's 1,420, without being
  fragmented; a fragmented packet is lost if either half is, and the board
  has to put it back together. Only the last packet has the push flag, so
  WLED shows the frame once all of it is in.

  | Packet | Offset | Length | Flags         |
  |--------|--------|--------|---------------|
  | 1–8    | 0, 1368, … 9576 | 1368 | `0x40` |
  | 9      | 10944  | 1344   | `0x41` (push) |

- Type `0x0B` (RGB, 8 bits), ID 1.
- The sequence number goes up once per frame, not per packet: all nine
  packets of a frame share it, running 1 to 15 and back to 1. This departs
  from the spec's per-packet wording; WLED's out-of-sequence check compares
  each packet with the last push, and one number per frame keeps a whole
  frame on the same side of it.
- The socket is unconnected on purpose: while the board reboots, frames
  are lost instead of the send failing and ending the run.
- Frames are sent at `fps` (10 by default) even when nothing changes,
  which keeps WLED in realtime mode.

With IP and UDP headers a frame is 12,630 bytes on the network, so 10 fps
is 90 packets and about 126 KB a second (1 Mbit/s); 30 fps is about
3 Mbit/s.

## Limitations

- **Pixels only.** DDP carries bytes at offsets. Layout, regions, text,
  fonts, images and blending all happen in the sender. For flexible
  regions this is good news: the protocol puts no constraint on how the
  screen is divided, only the renderer does.
- **Regions are not contiguous.** Offsets are linear, so a rectangle
  narrower than the panel is one run per row (a 24×24 tile is 24 runs of
  72 bytes). Partial updates of a region are possible but cost one packet
  per row, or a packet spanning the unchanged pixels between rows. And
  since WLED ignores custom IDs, a region cannot be addressed by an ID of
  its own.
- **No delivery guarantee.** UDP with no acknowledgement or retransmission.
  A lost data packet leaves that part of the buffer holding the previous
  frame; a lost push leaves the whole frame unshown until the next push.
  Sending every frame whole means any loss is repaired on the next frame,
  a tenth of a second later. Wi-Fi loses more than cable.
- **No feedback.** WLED sends nothing back: no discovery, no status, no
  size, no frame rate. The sender has to be told the matrix size and
  cannot tell whether frames are shown, dropped or too fast for the board.
- **No flow control.** Nothing slows a sender down. The limit on frame
  rate is the board (an ESP32 over Wi-Fi) and the network, not the
  protocol, which could carry far more.
- **8 bits per channel.** WLED takes RGB or RGBW at 8 bits; the 16-bit
  types in the spec are not supported. HSL and greyscale would be read as
  RGB.
- **Must keep sending.** A pause longer than the realtime timeout drops the
  board back to its presets, so even a still picture has to be resent.
- **No synchronisation beyond push.** WLED ignores timecodes; with one
  board this does not matter.
- **One sender at a time.** Two senders both write the buffer and push, so
  their frames interleave and flicker.
- **No security.** Anything on the network can send to port 4048.
- **Payload per packet.** 1,440 bytes, 480 RGB pixels, so a 64×64 frame
  always takes nine packets; a larger matrix takes more, and a 128×128
  panel would need 35 per frame.
- **Fragmentation.** A datagram bigger than a link's MTU is split into IP
  fragments, and losing any one loses the packet. A full DDP packet is
  1,478 bytes on the wire; a VPN tunnel typically carries 1,420 at most.
  `panel-ddp` keeps under 1,406, but a board reached through a tunnel still
  gets its latency and jitter: best on the same network, or with the sender
  beside it.

## References

- DDP specification, 3waylabs: <http://www.3waylabs.com/ddp/>
- WLED's receiver: `wled00/e131.cpp` (`handleDDPPacket`) and the constants
  in `wled00/src/dependencies/e131/ESPAsyncE131.h`, at
  <https://github.com/wled/WLED>
- The sender here: `src/ddp.rs`
