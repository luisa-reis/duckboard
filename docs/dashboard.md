# Dashboard over DDP

`panel-ddp` is a Rust program that draws a dashboard for a WLED matrix (64×64
unless the config says otherwise)
and streams it to the board over DDP (Distributed Display Protocol, UDP port 4048), which
WLED listens on out of the box. Nothing is installed on the board: WLED shows
what arrives as realtime input and drops back to its presets a couple of
seconds after the stream stops, so the GIF playlist is the fallback whenever
the sender is off. The gap file applies to streamed frames too. How the
screen is split into regions, and what each shows when, is set by one YAML
file, below. How DDP works and what it cannot do is in [ddp.md](ddp.md).

## The configuration file

One YAML file drives the panel, `dashboard.yaml` by default: copy
`dashboard.example.yaml` and edit it. It names its parts and refers to them
by name:

- **layouts** — named regions of the panel, each an `x`, `y`,
  `width` and `height` in pixels and a `z` (default 0);
- **tiles** — what a region shows: a kind and its settings, and optionally
  a colour `scheme` and `colors` of its own;
- **pages** — a layout, the tile for each region it fills (by name, or
  spelt out inline), how long it shows, and optionally a scheme and
  made-up `data`;
- **playlists** and a **schedule** — which pages play when;
- **schemes** — colour schemes by name.

```yaml
layouts:
  classic:
    background: {x: 0, y: 0, width: 64, height: 64, z: -1}
    top_left: {x: 2, y: 2, width: 24, height: 24}
    hub: {x: 21, y: 21, width: 22, height: 22, z: 1}
tiles:
  clock: {kind: clock}
  cover: {kind: art, shape: disc}
pages:
  home:
    layout: classic
    tiles: {top_left: clock, hub: cover, bottom_left: {kind: date}}
```

`panel-ddp.schema.json` describes every setting; editors that read the
`# yaml-language-server: $schema=panel-ddp.schema.json` line at the top of
the file check it as you type, and `panel-ddp check dashboard.yaml` loads it
and names what is wrong, page and region included. A running panel picks a
saved change up within a second.

### Regions and layers

Regions may overlap. They are drawn in ascending `z`, and those with the
same `z` in the layout's order, so a higher one draws over a lower one. A
region has no background of its own: it covers what is below only where it
draws. A full-panel region at the lowest `z` holding an `art` or `picture`
tile with an `alpha` is a background. Regions left empty on a page draw
nothing.

A tile lays its content out for 24 pixels of height, centred in a taller or
shorter region, and takes the region's width: the clock's ring is the
largest circle that fits, text is centred and fitted to the width, the
progress bar spans it. Fonts do not scale, and what does not fit is clipped
to the region. Album art and pictures are decoded at every size they show
at, and the disc is the largest circle that fits.

### Tile kinds

No source needed:

- `clock` — hours over minutes inside a seconds ring, advanced once a
  second: `seconds: dot` (the default) moves a dot of `dot_size` ring
  pixels (default 2) round it like a second hand, `seconds: ring` fills
  it clockwise from twelve.
- `date` — weekday, day of month, month.
- `text` — one line, `text`, centred in the region and scrolling at 5
  pixels a second when wider than it. `size` is the font, by the width and
  height of a character in pixels: `4x6`, `5x7`, `5x8`, `6x9`, `6x10` (the
  default), `6x12`, `6x13`, `7x13`, `7x14`, `8x13`, `9x15`, `9x18` or
  `10x20`. It is drawn in the `text` colour. The fonts cover Latin-1; any
  other character shows as `?`. `demo-text.yaml` shows every size.
- `blank`

With `sources.weather`, which is just a location:

- `weather` — sky icon and temperature from Open-Meteo, no key needed.

With `sources.spotify`, or a Home Assistant `media_player`:

- `now_playing` — artist and title, scrolling at 5 pixels a second when
  wider than the tile, blank while nothing plays.
- `art` — the album art. `shape` is `disc` (a record with a spindle hole,
  the default), `square` (the whole cover) or `faded` (the whole cover with
  the corners outside the circle dimmed at `corner_alpha`, default 0.3);
  `spin: true` turns it while playing, once every 8 seconds;
  `paused_alpha` (default 0.4) dims it while paused; `alpha` (default 1)
  blends it over what is under it; `idle` is `ripple` (the default, a faint
  ripple from the middle, a step every 0.4 seconds, while there is no art)
  or `none`.

Every animation keeps its speed at any `fps`: the scrolling, the spin, the
ripple, the seconds ring, sweeping page values and the alert pulse all go by
the time, so a higher frame rate only makes them smoother.

With `sources.home_assistant`:

- `sensor` — one entity: label, value, unit. A numeric value loses decimals,
  then switches to the small font, to fit four characters.
- `progress` — one entity as a label, the value with its unit beside it, and
  a bar that is full at `max` (default 100), amber on the way and green when
  full.
- `line_chart` — one `entity`'s numeric history over the last `hours`
  (default 24, up to 720) as a line across the region, the lowest value on the
  bottom row and the highest on the top one, with nothing else: no axes
  and no labels. `line` is its colour (the `accent` colour unless set);
  `dot` puts a three-pixel dot of that colour on the latest value, and
  without it there is none. The history is fetched once a minute; states
  that are not numbers are skipped, and a flat line in the `track` colour
  shows while there is nothing to draw. Text tiles beside it name it and
  give the value; `demo-dashboards.yaml` has examples.
- `area_chart` — a `line_chart` with the area under the line filled, down
  to the bottom of the region; it takes the same settings. `area` is the
  area's colour, the line's at a third of its strength unless set.
  `area_bottom` makes it a gradient, from `area` on the region's top row to
  `area_bottom` on its bottom row; a colour with an alpha of `00` fades it
  out. The line is drawn over the area, so an alpha in `line` blends with
  it.

With `sources.http`:

- `line_chart` or `area_chart` with a `series` in place of the `entity` —
  the same chart, of values pushed to the panel under that name; see
  Pushing a series.

With `sources.pictures`:

- `picture` — the picture due, at `alpha` (default 1) over what is under it.

A tile whose source is not configured is refused, unless its page brings
the data itself (see Pages below). When both Spotify and a Home Assistant
media player are set, Spotify feeds the art.

![sample dashboard behind the mask](dashboard-preview.png)

### Just the art

A page whose only tile is the art, square, across the panel, turns it into
a cover display; `paused_alpha: 1.0` keeps it from dimming while paused:

```yaml
layouts:
  full: {all: {x: 0, y: 0, width: 64, height: 64}}
tiles:
  cover: {kind: art, shape: square, paused_alpha: 1.0, idle: none}
pages:
  art: {layout: full, tiles: {all: cover}}
```

## Building

The crate pins its toolchain in `rust-toolchain.toml`; rustup installs
it on the first `cargo` run inside this repository and it applies only there. The
machine default (1.81 at the time of writing) cannot parse the manifests of
current crates.

```sh
cargo build --release
```

## Installing

The sender needs only the network: it can live on a Raspberry Pi next to
the panel or run in the background on a laptop. Relative paths in a config
(`gaps`, `secrets`, the token file, `art-cache`, the pictures) resolve
against the config's own folder, so a service only has to pass `--config`
with an absolute path. A secret can also come from the service's
environment, `PANEL_DDP_SECRET_HOME_ASSISTANT_TOKEN` for
`{secret: home_assistant_token}`, for example from systemd's
`EnvironmentFile=`.

### Raspberry Pi

Cross-compile on the laptop rather than building on the Pi. `ring` (under
`rustls`) compiles C, so the target needs a C cross-compiler;
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) uses Zig for
that and needs no Docker. For 64-bit Raspberry Pi OS:

```sh
brew install zig
cargo install --locked cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu   # inside the repo: adds it to the pinned toolchain
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.31
# -> target/aarch64-unknown-linux-gnu/release/panel-ddp
```

The `.2.31` suffix links against glibc 2.31, so the binary runs on Bullseye
and later. On 32-bit Raspberry Pi OS use `armv7-unknown-linux-gnueabihf`
instead. [cross](https://github.com/cross-rs/cross)
(`cross build --release --target aarch64-unknown-linux-gnu`) does the same
through Docker.

Copy the binary, the config, the secrets file and any Spotify token file
over (the gap file is only for previews), and keep the secrets private:

```sh
ssh pi mkdir -p panel-ddp
scp target/aarch64-unknown-linux-gnu/release/panel-ddp \
    dashboard.yaml secrets.yaml spotify-token.json pi:panel-ddp/
ssh pi chmod 600 panel-ddp/secrets.yaml
```

Do `spotify-login` on the laptop, where a browser can reach the redirect,
and copy the token file; from then on the Pi rewrites it as Spotify rotates
it. Run it as a systemd service, `/etc/systemd/system/panel-ddp.service`:

```ini
[Unit]
Description=panel-ddp dashboard
Wants=network-online.target
After=network-online.target

[Service]
User=pi
ExecStart=/home/pi/panel-ddp/panel-ddp run --config /home/pi/panel-ddp/dashboard.yaml
KillSignal=SIGINT
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now panel-ddp
journalctl -u panel-ddp -f          # the log
```

`KillSignal=SIGINT` makes `systemctl stop` end the run the way Ctrl-C does,
so it tidies up.

### In the background on a laptop

On macOS a launchd agent keeps it running while logged in,
`~/Library/LaunchAgents/panel-ddp.plist` (use absolute paths; launchd does
not expand `~`):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>panel-ddp</string>
  <key>ProgramArguments</key>
  <array>
    <string>/path/to/panel-ddp/target/release/panel-ddp</string>
    <string>run</string>
    <string>--config</string>
    <string>/path/to/panel-ddp/dashboard.yaml</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>/tmp/panel-ddp.log</string>
</dict>
</plist>
```

```sh
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/panel-ddp.plist
launchctl bootout gui/$(id -u)/panel-ddp     # stop it
```

On a Linux laptop, the systemd unit above works as a user service
(`~/.config/systemd/user/`, without `User=`, `WantedBy=default.target`,
managed with `systemctl --user`). For a one-off, `nohup
target/release/panel-ddp run > panel-ddp.log 2>&1 &` is enough.

## Settings

`target` is the board; the rest is optional:

- `width` and `height` (default 64 each, up to 1024) — the panel, in
  pixels. WLED on the board must be set up as a 2D matrix of the same size:
  DDP has no way to tell the sender the board's size, so a mismatch comes
  out scrambled or cut off. Every region must fit, and a frame takes more
  packets on a bigger panel (18 for 128×64). Changed while running, they
  take effect at the next start.
- `fps` (default 10) — frames per second.
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
  pixels the panel hides; it must have a value for every pixel. Without it
  the preview shows the whole panel.
  `2d-gaps.json` is git-ignored; `2d-gaps.example.json` is a sample that
  hides the four corners.
- `secrets` (default `secrets.yaml`) — the secrets file, below.
- `sources.weather` — `latitude`, `longitude`, `refresh_minutes`, and
  `units` to override `temperature` for the weather tile alone.
- `sources.spotify` — `client_id`, and optionally `token_file` (default
  `spotify-token.json` next to the config) and `refresh_seconds`. See below.
- `sources.home_assistant` — `url`, the long-lived access `token` (profile
  page, Security tab) as a secret, the `media_player` entity to use as the
  media source when there is no Spotify, `refresh_seconds`.
- `sources.http` — `listen`, the address and port the series of charts
  are pushed to (default `127.0.0.1:4049`), and optionally a `token`, as a
  secret, that every request must carry; see Pushing a series.
- `sources.pictures` — `dir` (default `frame`), `seconds` each picture
  shows (default 10) and `shuffle`; see Picture frame.
- `art_cache` — `dir` (default `art-cache` next to the config),
  `max_megabytes` (default 4; 0 disables) and `keep_originals` (default
  false). See Running.
- `page_seconds` (default 10) — how long a page shows unless it says.
- `alert_area` — where an alert centres its label (default a 22×22 square
  in the middle of the panel).

### Secrets

The config never holds a secret: it names one, `token: {secret:
home_assistant_token}`, and the value comes from the environment variable
`PANEL_DDP_SECRET_HOME_ASSISTANT_TOKEN` when that is set, and otherwise from
the secrets file, a map of names to values:

```yaml
home_assistant_token: "eyJhbGciOi..."
```

`secrets.yaml` is git-ignored; keep it readable by its owner alone (`chmod
600`), which loading checks. `secrets.example.yaml` shows the shape. So the
config itself can be committed, shared and edited without exposing
anything.

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

`schemes` sets roles by name. The `default` scheme applies to every tile;
a page's `scheme` goes over it for that page, a tile's `scheme` over that,
and a tile's own `colors` last. A red accent everywhere, a dim night page,
and the clock's ring at a quarter alpha with green minutes:

```yaml
schemes:
  default: {accent: "#ff4060"}
  dim: {text: "#606060", label: "#303030"}
tiles:
  clock: {kind: clock, colors: {track: "#ffffff40", secondary: "#80ff80"}}
pages:
  night: {layout: big-clock, scheme: dim, tiles: {clock: clock}}
```

## Pushing a series

A line chart or an area chart with a `series` draws values sent to the panel over HTTP, for
anything Home Assistant does not hold: a script's output, a build's
duration, a price.

```yaml
sources:
  http: {listen: "127.0.0.1:4049"}
tiles:
  power: {kind: line_chart, series: power, dot: "#ffffff"}
```

```sh
curl -X PUT  -d '[412, 398, 455, 620]' http://127.0.0.1:4049/series/power   # the whole line
curl -X POST -d '640'                  http://127.0.0.1:4049/series/power   # one more value on its end
curl                                   http://127.0.0.1:4049/series/power   # what it holds
curl -X DELETE                         http://127.0.0.1:4049/series/power   # empty it
```

- `PUT` replaces the series with the body, a JSON array of numbers, oldest
  first; `POST` adds a number, or an array of them, to its end. A series
  keeps its newest 1024 values. The answer is the count, `4 values`.
- Only the series a chart in the config names are taken; any other
  name is a 404, as is any other path. A body that is not numbers is a 400
  and changes nothing.
- A name is letters, digits, `.`, `_` and `-`.
- The values are kept in memory: a reload of the config keeps them, and
  they are gone when the program restarts.
- `listen` defaults to this machine only. `0.0.0.0:4049` takes requests
  from the network; without a token, anything that reaches the port can
  change the lines.
- `token: {secret: http_token}` asks every request for that secret as a
  bearer token, and answers 401 without it. It goes over plain HTTP, so it
  keeps out what merely reaches the port, not what can read the traffic.

  ```yaml
  sources:
    http: {listen: "0.0.0.0:4049", token: {secret: http_token}}
  ```

  ```sh
  curl -X POST -d '640' -H "Authorization: Bearer $TOKEN" http://panel.local:4049/series/power
  ```
- The endpoint has its own thread and answers each request on another, so
  a slow or stuck client never delays a frame.

## Picture frame

The pictures in a folder, one after another, each cropped to its area,
scaled and gamma-corrected like album art. A picture frame is a page with a
`picture` tile across the panel and nothing over it; at a low `alpha` under
other tiles, the pictures are a background. The folder and pacing are in
`sources.pictures`, with these defaults:

```yaml
sources:
  pictures:
    dir: frame        # relative to the config; .jpg, .jpeg and .png files
    seconds: 10       # each picture
    shuffle: false    # random order, reshuffled at each start; else by file name
```

Pictures with a bright subject on black suit the panel best. `frame/` is
git-ignored; keep a note of where each picture came from and its licence
beside them, as the `SOURCES.md` written there does.

## Pages, playlists and the schedule

Pages show one after another, each for its `seconds` (or `page_seconds`).
`playlists` group them by name, and the `schedule` picks which playlist
plays: its rules are checked in order at the end of every page, and the
first that matches wins. A rule's `when` limits it to `days` (`mon` …
`sun`) and a window `from` … `to` (`"HH:MM"`, local time; `to` earlier than
`from` runs past midnight); a rule without `when` always matches. A change
of playlist starts the new one at its first page. When no rule matches,
nothing is sent and the board falls back to its presets. Without
`playlists`, every page plays in file order; without a `schedule`, the first
playlist always plays.

```yaml
playlists:
  day: [home, music]
  night: [night-clock]
schedule:
  - playlist: night
    when: {from: "23:00", to: "07:00"}
  - playlist: day
```

`--once` plays the playlist on at the start through a single time.

A page's `data` lays made-up values over whatever the sources report, for
demos and for pinning a page to something no source provides; a tile whose
source is missing is allowed when its page brings the data:

- `weather: {code, is_day, temperature}` — a WMO code, in the configured
  temperature unit.
- `sensors: {sensor.x: {state: "29", unit: "°C"}}`, or `{sweep: [0, 100]}`
  to move the value linearly across the page. An alert's entity set to its
  state raises the alert.
- `series: {sensor.x: [3, 4, 6, 5]}` — the values a chart of the
  entity, or of the series of that name, draws, oldest first, whatever its
  `hours`.
- `cover: N` — the Nth newest cover in the art cache, playing. With
  `keep_originals`, only covers with an original count.
- `picture: N` — the Nth picture, for a picture tile.

A page naming a cover or picture that is not there is left out, with a note.

`demo.yaml` is the demo built this way: the date alone touring the tiles,
the clock joining, every kind of sky, the print progress filling, the
laundry temperature in its place, the water leak alert, the whole dashboard
plain, a cover in the hub, the dashboard over each cover, the covers alone,
the pictures full screen, and the dashboard over them. Its timings, labels
and order are all in the file. Its `target` is the WLED-AP address; point
it at a board with `--target`:

```sh
target/release/panel-ddp run --config demo.yaml --target wled.local          # loops
target/release/panel-ddp run --config demo.yaml --target wled.local --once   # one pass
```

`demo-text.yaml` is the text demo: every size of the `text` tile, each line
in its own size and saying which, then lines too wide for the panel,
scrolling. It needs no data.

`demo-dashboards.yaml` is the dashboards demo: six dashboards built from
line charts, area charts and text of different sizes on made-up series: one
reading with its day under it, three readings each beside its chart, the
clock and the date over a reading, text over a chart the size of the panel,
and then area charts, one with a gradient under a reading and three beside
theirs.

`art_file`, a top-level setting, keeps a JPEG at the album cover on show,
the original as downloaded, black when no cover is on, and removes it when
the run ends; `demo.yaml` sets it to `demo-art.jpg`. `art_open: true` runs
`open` on it after each change for macOS Preview. `tools/DemoArtViewer` is
a Processing sketch that follows the file without that.

## Alerts

An alert is a Home Assistant entity and the state that raises it. While
any alert is raised, the panel drops the page, pulses in the alert's colour
and shows its label on `alert_area`, then returns to the page when the
state clears. Any number of alerts; the first raised one wins:

```yaml
alerts:
  - entity: binary_sensor.bathroom_water_leak
    state: "on"             # the default; quoted, as YAML reads a bare on as true
    label: WATER LEAK       # up to four characters large, up to eleven small along the top of the area
    color: "#ff1e1e"        # the default
    pulse_seconds: 1.5      # the default
```

Alerts need `sources.home_assistant`, unless a page gives their entity's
state; their entities are polled with the sensors.
`preview --alert` renders one, and `run --sample` raises them for five
seconds of every thirty.

## Spotify

Spotify's API needs an app of your own, which takes a minute: at
developer.spotify.com/dashboard create an app: any name and description, the
redirect URI `http://127.0.0.1:8888/callback`, and under "Which API/SDKs are
you planning to use?" tick Web API only. Copy its Client ID into
`sources.spotify.client_id` (it is not a secret). Then log in once:

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
target/release/panel-ddp run                        # dashboard.yaml, until Ctrl-C
target/release/panel-ddp run --config other.yaml --frames 100
target/release/panel-ddp run --sample                 # made-up data, sensors sweep 0..100: a demo of the layout
target/release/panel-ddp run --config demo.yaml --target <board>   # the demo; --once for a single pass
target/release/panel-ddp preview --out preview.png  # the first page from sample data, mask applied
target/release/panel-ddp preview --out preview.gif  # the first page animated, for its time; --seconds N for longer or shorter
target/release/panel-ddp preview --out preview.apng # the same as an animated PNG, in full colour (GIF has 256 a frame)
target/release/panel-ddp preview --weather-code 95  # check an icon (add 1000 for night)
target/release/panel-ddp preview --alert            # the alert view
target/release/panel-ddp test <board-ip>            # colour bars, ramp, counter, bouncing dot; --size 128x64 for another panel
target/release/panel-ddp render --config demo.yaml --out /tmp/r   # every frame's hash, no network
target/release/panel-ddp check dashboard.yaml demo.yaml   # load each, say what it holds or what is wrong
target/release/panel-ddp schema                         # the YAML file's JSON Schema
target/release/panel-ddp migrate dashboard.toml         # write dashboard.yaml (token to secrets.yaml), checked to draw the same
```

`render` draws what `run` would send, frame by frame, without sending it or
fetching anything: the clock starts at a fixed time (`--at`, seconds since
1970) and the data is empty, so pages bring their own, or made up with
`--sample`. Without `--frames N` it renders one pass of the playlist on at
that time. It writes
`frames.txt`, one SHA-256 per frame, and a PNG of each frame named with
`--png N`. Rendering before and after a change and comparing the two lists
shows whether the change moved a single pixel.

Common patterns:

```sh
# One demo pass, then back to the live dashboard: --once makes the first
# command end, so the second takes over.
target/release/panel-ddp run --config demo.yaml --target <board> --once && \
  target/release/panel-ddp run --config dashboard.yaml

# Stop whatever is streaming, from another terminal. Ctrl-C does the same in
# its own; either way the run ends cleanly and removes its art_file. The
# board falls back to its presets a couple of seconds later.
pkill -f "panel-ddp run"
```

`run` checks about once a second whether its config file (or its secrets
file) has changed. A changed file that loads is
switched to at once, keeping the data sources running when their settings
did not change; one that does not load is reported, with why, and the
running config stays. `target` and `fps` take effect at the next start.

Only one sender at a time: two streams to the same board fight over the
panel, so stop the running one before starting another.

Each source runs on its own thread with its own refresh interval and keeps the
last good reading, so a slow or dead service never stalls a frame. A failure is
logged once, when its message changes. Album art is fetched only when the
picture URL changes.

Decoded album art is cached on disk under `art_cache.dir`, one file per
picture URL, gamma and the sizes it shows at, holding the pixels at each size (the hub and the panel), about 14 KB each,
so the default 4 MB cap holds a few hundred covers. A hit costs no download
and no decoding and makes the entry the newest; past the cap the oldest
files are deleted first. The directory is git-ignored under the crate.
With `keep_originals`, each picture is also kept as downloaded, as a `.jpg`
or `.png` file named `Artist - Album` when the source says what it is and by
the URL's hash otherwise, and Spotify is asked for its largest
size; the `art_file` is then written from
that at full size instead of the panel's pixels scaled up. Originals count
against the cap, a few tens of kilobytes each.

The sender uses an unconnected UDP socket on purpose: a connected one turns
the ICMP unreachable from a rebooting board into a send error, which would end
the stream. Unconnected, the frames are lost until the board is back.

## Migrating an older config

Configs used to be TOML (`dashboard.toml`) or the same in JSON
(`demo.json`). `run` and the other commands now refuse them and name the
command that converts one:

```sh
target/release/panel-ddp migrate dashboard.toml   # writes dashboard.yaml; --out FILE, --secrets FILE
```

The five `[regions]` become a layout named `classic`, with a `background`
region under them when a page has a background; each distinct tile gets a
name (its label for sensors and progress bars, else its kind); pages are
named `page-1` and on; settings left at their defaults are left out; and
the Home Assistant token moves to the secrets file. Before it finishes it
loads both files and checks that every page draws the same, and says so.

## What to check on the panel

The `test` frame's bars should come out red, green, blue, white left to right.
Mirrored output is the panel config's Reversed flag, not the sender. On the
SM16380SH batch, confirm the S-PWM engine keeps up at 10 fps; the engine is
ours, not the library's, and a full-frame rewrite ten times a second is more
than the GIF presets ask of it.
