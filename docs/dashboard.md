# Dashboard over DDP

`panel-ddp` is a Rust program that draws a dashboard for the panel and streams it
to the board over DDP (Distributed Display Protocol, UDP port 4048), which
WLED listens on out of the box. Nothing is installed on the board: WLED shows
what arrives as realtime input and drops back to its presets a couple of
seconds after the stream stops, so the GIF playlist is the fallback whenever
the sender is off. The gap file applies to streamed frames too.
The screen is split into areas, laid out below.

## Layout

The screen is split into five areas, four tiles and a hub:

| Area         | Origin   | Size  |
|--------------|----------|-------|
| top left     | (2, 2)   | 24×24 |
| top right    | (38, 2)  | 24×24 |
| bottom left  | (2, 38)  | 24×24 |
| bottom right | (38, 38) | 24×24 |
| hub          | (21, 21) | 22×22 |

Each corner shows one tile, the hub one of its own kinds:

- `clock` — hours over minutes, a seconds bar along the bottom edge.
- `date` — weekday, day of month, month.
- `weather` — sky icon and temperature from Open-Meteo, no key needed.
- `sensor` — one Home Assistant entity: label, value, unit. A numeric value
  loses decimals, then switches to the small font, to fit four characters.
- `progress` — one Home Assistant entity as a label, the value with its unit
  beside it, and a bar along the bottom edge that is full at `max` (default
  100), amber on the way and green when full.
- `now_playing` — artist and title of the media player, scrolling when wider
  than the tile, blank while nothing plays.
- `blank`
- hub `media` — the album art as a disc, spinning while playing, dimmed and
  still while paused, a faint ripple when idle.
- hub `blank`

![sample dashboard behind the mask](dashboard-preview.png)

## Building

The crate pins its toolchain in `rust-toolchain.toml`; rustup installs
it on the first `cargo` run inside this repository and it applies only there. The
machine default (1.81 at the time of writing) cannot parse the manifests of
current crates.

```sh
cargo build --release
```

## Configuration

Copy `dashboard.example.toml` to `dashboard.toml` (git-ignored; it
holds the Home Assistant token) and edit. `target` is the board, the rest is
optional:

- `gaps` (default `2d-gaps.json`) — a copy of the board's WLED gap file (one
  value per pixel, 1 for shown), used only by `preview` to grey out the
  pixels the panel hides. Without it the preview shows the whole 64×64.
  `2d-gaps.json` is git-ignored; `2d-gaps.example.json` is a sample that
  hides the four corners.
- `[weather]` — latitude, longitude, `units` (`celsius` or `fahrenheit`),
  `refresh_minutes`.
- `[home_assistant]` — `url`, a long-lived access `token` (profile page,
  bottom), the `media_player` entity for `now_playing` and the `media` hub,
  `refresh_seconds`.
- `[tiles]` — a tile per corner and one for the hub. A `sensor` tile names its
  `entity` and `label`, and may set `unit` (`""` hides it) and `decimals`. A
  `progress` tile names `entity` and `label`, and may set `max` and `decimals`.

Loading refuses a config whose tiles need a table it lacks.

## Running

```sh
target/release/panel-ddp run                        # dashboard.toml, until Ctrl-C
target/release/panel-ddp run --config other.toml --frames 100
target/release/panel-ddp run --sample                 # made-up data, sensors sweep 0..100: a demo of the layout
target/release/panel-ddp preview --out preview.png  # one frame from sample data, mask applied
target/release/panel-ddp preview --weather-code 95  # check an icon (add 1000 for night)
target/release/panel-ddp test <board-ip>            # colour bars, ramp, counter, bouncing dot
```

Each source runs on its own thread with its own refresh interval and keeps the
last good reading, so a slow or dead service never stalls a frame. A failure is
logged once, when its message changes. Album art is fetched only when the
picture URL changes.

The sender uses an unconnected UDP socket on purpose: a connected one turns
the ICMP unreachable from a rebooting board into a send error, which would end
the stream. Unconnected, the frames are lost until the board is back.

## What to check on the panel

The `test` frame's bars should come out red, green, blue, white left to right.
Mirrored output is the panel config's Reversed flag, not the sender. On the
SM16380SH batch, confirm the S-PWM engine keeps up at 10 fps; the engine is
ours, not the library's, and a full-frame rewrite ten times a second is more
than the GIF presets ask of it.
