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

- `clock` — hours over minutes inside a seconds ring, advanced once a
  second: `seconds = "dot"` (the default) moves a dot of `dot_size` ring
  pixels (default 2) round it like a second hand, `seconds = "ring"` fills
  it clockwise from twelve.
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

- `gamma` (default 2.2) — applied to pictures before they are sent. WLED
  gamma-corrects its own effects and GIFs but not streamed frames, and the
  HUB75 build defines `NO_CIE1931` so the driver is linear too, so an sRGB
  cover sent raw comes out washed out. Set 1.0 if the board's realtime gamma
  correction is switched on instead. The tiles' own colours are sent as they
  are; they were chosen on the panel.
- `temperature` (default `fahrenheit`, or `celsius`) — the unit for every
  temperature shown: the weather, and any sensor whose reading is in
  degrees, converted when Home Assistant reports the other unit.
- `gaps` (default `2d-gaps.json`) — a copy of the board's WLED gap file (one
  value per pixel, 1 for shown), used only by `preview` to grey out the
  pixels the panel hides. Without it the preview shows the whole 64×64.
  `2d-gaps.json` is git-ignored; `2d-gaps.example.json` is a sample that
  hides the four corners.
- `[weather]` — latitude, longitude, `refresh_minutes`, and `units` to
  override `temperature` for the weather tile alone.
- `[spotify]` — `client_id`, and optionally `token_file` (default
  `spotify-token.json` next to the config) and `refresh_seconds`. See below.
- `[art_cache]` — `dir` (default `art-cache` next to the config),
  `max_megabytes` (default 4; 0 disables) and `keep_originals` (default
  false). See below.
- `[home_assistant]` — `url`, a long-lived access `token` (profile page,
  Security tab), the `media_player` entity to use as the media source when
  there is no `[spotify]`, `refresh_seconds`.
- `[tiles]` — a tile per corner, one for the hub, and the `background`. A `sensor` tile names its
  `entity` and `label`, and may set `unit` (`""` hides it) and `decimals`. A
  `progress` tile names `entity` and `label`, and may set `max` and `decimals`.

Loading refuses a config whose tiles need a table it lacks. When both
`[spotify]` and a Home Assistant media player are set, Spotify feeds the hub.

## Colours

Every colour the tiles draw with has a role, and every role can be set in
the config as `#rrggbb` or `#rrggbbaa`. An alpha below `ff` blends the
colour over whatever is already on the panel at that spot, the background
art or black, so a translucent track lets the cover show through it. The
roles and their defaults, which are the values the tiles were tuned with:

| role          | default   | used for                                           |
|---------------|-----------|----------------------------------------------------|
| `text`        | `#ffffff` | hours, the day, sensor readings, the title         |
| `label`       | `#6e6e6e` | labels, units, the month, the artist, a bar's frame |
| `track`       | `#2d2d2d` | the unfilled seconds ring, an empty bar, `--`      |
| `accent`      | `#ffaa00` | the weekday, the seconds fill, a bar on its way    |
| `secondary`   | `#50aaff` | the minutes                                        |
| `full`        | `#3cdc5a` | a bar at its maximum                               |
| `sun`         | `#ffaa00` | the sun, and lightning                             |
| `moon`        | `#dcdcb4` | the moon                                           |
| `cloud`       | `#c8c8d2` | clouds                                             |
| `storm_cloud` | `#78788c` | the storm cloud                                    |
| `rain`        | `#3c8cff` | rain                                               |
| `snow`        | `#ffffff` | snow                                               |
| `fog`         | `#6e6e6e` | fog                                                |

`[colors]` sets a role for every tile; a tile's own `colors` sets it for
that tile alone and wins. The seconds ring's track at a quarter alpha, and
green minutes, on the clock only:

```toml
[colors]
accent = "#ff4060"

[tiles]
top_left = { kind = "clock", colors = { track = "#ffffff40", secondary = "#80ff80" } }
```

The hub's `media` kind has two alphas of its own, `paused_alpha` (default
0.4) for the art while paused and `corner_alpha` (default 0.3) for the
corners of the faded shape; the background has its `alpha`.

## The demo

`panel-ddp demo` streams a scripted crescendo on made-up data, for showing
the panel off: the date alone, visiting each tile in turn; the clock joining
it; the weather tile showing every kind of sky, day and night; the print
progress filling from 0 to 100; the laundry temperature in its place; the
water leak alert; the whole dashboard plain for a moment; the same with a cover as a disc in the hub; the same
over each album cover from the art cache, newest first; and finally the
covers alone, nothing else drawn. It loops until
Ctrl-C, or `--once` plays a single pass. It needs no source: only `target`,
and a populated art cache for the last step, which is skipped when empty.
The alert takes its look from the config's first `[[alerts]]` entry.

Every timing is in `[demo]`, in seconds, so the pacing is tuned without a
rebuild. The defaults:

```toml
[demo]
tile_seconds = 3.0        # the date in each tile
clock_seconds = 5.0       # after the clock joins
weather_seconds = 2.5     # each sky, nine of them
progress_seconds = 10.0   # the bar filling
sensor_seconds = 4.0      # the laundry temperature in its place
alert_seconds = 5.0       # the alert
dashboard_seconds = 5.0   # the dashboard after it, plain
hub_seconds = 5.0         # a cover as a disc in the hub
cover_seconds = 4.0       # each cover behind the tiles
art_only_seconds = 4.0    # each cover alone, at the end
covers = 8                # how many covers, newest first
background_alpha = 0.12   # the covers behind the tiles
```

## Alerts

An alert is a Home Assistant entity and the state that raises it. While
any alert is raised, the panel drops the tiles, pulses in the alert's colour
and shows its label in the middle, then returns to the dashboard when the
state clears. Any number of `[[alerts]]` tables; the first raised one wins:

```toml
[[alerts]]
entity = "binary_sensor.bathroom_water_leak"
state = "on"                # the default
label = "WATER LEAK"        # up to four characters large in the hub, up to eleven small above it
color = "#ff1e1e"           # the default
pulse_seconds = 1.5         # the default
```

Alerts need `[home_assistant]`; their entities are polled with the sensors.
`preview --alert` renders one, and `run --sample` raises them for five
seconds of every thirty.

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
target/release/panel-ddp demo                         # the scripted crescendo, looping; --once for a single pass
target/release/panel-ddp preview --out preview.png  # one frame from sample data, mask applied
target/release/panel-ddp preview --weather-code 95  # check an icon (add 1000 for night)
target/release/panel-ddp preview --alert            # the alert view
target/release/panel-ddp test <board-ip>            # colour bars, ramp, counter, bouncing dot
```

Each source runs on its own thread with its own refresh interval and keeps the
last good reading, so a slow or dead service never stalls a frame. A failure is
logged once, when its message changes. Album art is fetched only when the
picture URL changes.

Decoded album art is cached on disk under `[art_cache].dir`, one file per
picture URL and gamma holding the hub-sized and panel-sized pixels, about 14 KB each,
so the default 4 MB cap holds a few hundred covers. A hit costs no download
and no decoding and makes the entry the newest; past the cap the oldest
files are deleted first. The directory is git-ignored under the crate.
With `keep_originals`, each picture is also kept as downloaded, as a `.jpg`
or `.png` file named `Artist - Album` when the source says what it is and by
the URL's hash otherwise, and Spotify is asked for its largest
size; the demo's `art_file` is then written from
that at full size instead of the panel's pixels scaled up. Originals count
against the cap, a few tens of kilobytes each.

The sender uses an unconnected UDP socket on purpose: a connected one turns
the ICMP unreachable from a rebooting board into a send error, which would end
the stream. Unconnected, the frames are lost until the board is back.

## What to check on the panel

The `test` frame's bars should come out red, green, blue, white left to right.
Mirrored output is the panel config's Reversed flag, not the sender. On the
SM16380SH batch, confirm the S-PWM engine keeps up at 10 fps; the engine is
ours, not the library's, and a full-frame rewrite ten times a second is more
than the GIF presets ask of it.
