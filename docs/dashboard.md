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

Each corner shows one tile, the hub one of its own kinds. A clock-and-weather
panel needs no account at all; the rest switch on with a table in the config.

No source needed:

- `clock` — hours over minutes, a seconds bar along the bottom edge.
- `date` — weekday, day of month, month.
- `blank`

With `[weather]`, which is just a location:

- `weather` — sky icon and temperature from Open-Meteo, no key needed.

With `[spotify]`, or a Home Assistant media player:

- `now_playing` — artist and title, scrolling when wider than the tile, blank
  while nothing plays.
- hub `media` — the album art in the hub instead of the background, dimmed
  while paused, a faint ripple when nothing plays. `shape` is `disc` (a
  record with a spindle hole, the default), `square` (the whole cover) or
  `faded` (the whole cover with the corners outside the circle dimmed);
  `spin = true` turns it while playing.

With `[home_assistant]`:

- `sensor` — one entity: label, value, unit. A numeric value loses decimals,
  then switches to the small font, to fit four characters.
- `progress` — one entity as a label, the value with its unit beside it, and
  a bar along the bottom edge that is full at `max` (default 100), amber on
  the way and green when full.

The hub also takes `blank`.

The art shows in one place. By default it is the `background`: the cover
across the whole panel behind the tiles, blended over black at `alpha`
(default 0.12) so they stay legible, and nothing else done to it. A hub set
to `media` moves it there and leaves the background black. Setting both is
refused; `background = { kind = "none" }` shows no art at all.

![sample dashboard behind the mask](dashboard-preview.png)

### Just the art

Every tile and the hub `blank`, and the background at full brightness, turn
the panel into a cover display: the current album fills the center of the
display and nothing is drawn over it. Paused playback still dims it. As a second config
next to the dashboard, sharing the same Spotify app and token file:

```toml
target = "wled.local"

[spotify]
client_id = "..."

[tiles]
top_left = { kind = "blank" }
top_right = { kind = "blank" }
bottom_left = { kind = "blank" }
bottom_right = { kind = "blank" }
hub = { kind = "blank" }
background = { kind = "media", alpha = 1.0 }
```

```sh
target/release/panel-ddp run --config dashboard-art.toml
```

The same shape with a Home Assistant media player instead of Spotify: the
`[home_assistant]` table with `media_player` in place of `[spotify]`.

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
- `[spotify]` — `client_id`, and optionally `token_file` (default
  `spotify-token.json` next to the config) and `refresh_seconds`. See below.
- `[art_cache]` — `dir` (default `art-cache` next to the config) and
  `max_megabytes` (default 4; 0 disables). See below.
- `[home_assistant]` — `url`, a long-lived access `token` (profile page,
  Security tab), the `media_player` entity to use as the media source when
  there is no `[spotify]`, `refresh_seconds`.
- `[tiles]` — a tile per corner, one for the hub, and the `background`. A `sensor` tile names its
  `entity` and `label`, and may set `unit` (`""` hides it) and `decimals`. A
  `progress` tile names `entity` and `label`, and may set `max` and `decimals`.

Loading refuses a config whose tiles need a table it lacks. When both
`[spotify]` and a Home Assistant media player are set, Spotify feeds the hub.

## Spotify

Spotify's API needs an app of your own, which takes a minute: at
developer.spotify.com/dashboard create an app: any name and description, the
redirect URI `http://127.0.0.1:8888/callback`, and under "Which API/SDKs are
you planning to use?" tick Web API only. Copy its Client ID into `[spotify]`.
Then log in once:

```sh
target/release/panel-ddp spotify-login        # --port N if 8888 is taken; register that URI instead
```

It opens Spotify's consent page (or prints the link), catches the redirect
on the local port, and writes the token file next to the config. The flow is
PKCE, so there is no client secret anywhere. Spotify rotates the refresh
token on every use, so the file is rewritten as the dashboard runs; keep it
private and out of version control (the crate's `.gitignore` covers the
default name). If it is lost, log in again.

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

Decoded album art is cached on disk under `[art_cache].dir`, one file per
picture URL holding the hub-sized and panel-sized pixels, about 14 KB each,
so the default 4 MB cap holds a few hundred covers. A hit costs no download
and no decoding and makes the entry the newest; past the cap the oldest
entries are deleted first. The directory is git-ignored under the crate.

The sender uses an unconnected UDP socket on purpose: a connected one turns
the ICMP unreachable from a rebooting board into a send error, which would end
the stream. Unconnected, the frames are lost until the board is back.

## What to check on the panel

The `test` frame's bars should come out red, green, blue, white left to right.
Mirrored output is the panel config's Reversed flag, not the sender. On the
SM16380SH batch, confirm the S-PWM engine keeps up at 10 fps; the engine is
ours, not the library's, and a full-frame rewrite ten times a second is more
than the GIF presets ask of it.
