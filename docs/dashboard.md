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

[charts.md](charts.md) has the text, chart and table kinds at length, with
an example of each, and [designing-dashboards.md](designing-dashboards.md)
how to lay a page out and check it, by hand or with an agent.

No source needed:

- `clock` — hours over minutes inside a seconds ring, advanced once a
  second: `seconds: dot` (the default) moves a dot of `dot_size` ring
  pixels (default 2) round it like a second hand, `seconds: ring` fills
  it clockwise from twelve.
- `date` — weekday, day of month, month.
- `text` — one line, `text` (or a `column` of pushed data, on a table's
  repeat row; see Tables). `align` sets it against the `left` or the
  `right` of the region, or in its `center` (the default). Wider than the
  region, it scrolls at 5 pixels a second whatever the alignment; with
  `overflow: truncate` it is cut to the whole characters that fit instead,
  and aligned. `size` is the font, by the width and
  height of a character in pixels: `4x6`, `5x7`, `5x8`, `6x9`, `6x10` (the
  default), `6x12`, `6x13`, `7x13`, `7x14`, `8x13`, `9x15`, `9x18` or
  `10x20`. It is drawn in the `text` colour. The fonts cover Latin-1; any
  other character shows as `?`. `docs/demos/text.yaml` shows every size.
- `blank`
- `table` — rows of other tiles; see Tables.

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
  give the value; `docs/demos/line-charts.yaml` has examples.
- `area_chart` — a `line_chart` with the area under the line filled, down
  to the bottom of the region; it takes the same settings. `area` is the
  area's colour, the line's at a third of its strength unless set.
  `area_bottom` makes it a gradient, from `area` on the region's top row to
  `area_bottom` on its bottom row; a colour with an alpha of `00` fades it
  out. The line is drawn over the area, so an alpha in `line` blends with
  it.
- `bar_chart` — the same values as bars: as many as fit across the region
  at `width` pixels each (default 2) and `gap` pixels apart (default 1),
  centred, each the mean of its stretch of the values. `bar` is their
  colour (the `accent` colour unless set) and `last` that of the latest
  bar, when it should stand out. The bottom of the region is zero and the
  highest value reaches its top, so the bars compare by height; with a
  negative value among them the bottom is the lowest value instead. A bar
  is never under a pixel, so a zero still shows where it is. It takes
  `entity` or `series`, and `hours`, like a line chart.

- `bullet_chart` — a bullet graph, after Stephen Few: the entity's present
  state as a bar along a scale from `min` (default 0) to `max` (default
  100), a marker at `target`, and behind them bands that end at each of
  `ranges` (at most four), the first the strongest. `bar`, `marker` and
  `band` are their colours (the `accent`, `text` and `label` roles unless
  set). On a table's repeat row it takes `column`, and `target_column`,
  instead. See [charts.md](charts.md).

With `sources.http`:

- `bullet_chart` with a `series` — the latest value pushed under that name.
- `line_chart`, `area_chart` or `bar_chart` with a `series` in place of the
  `entity` —
  the same chart, of values pushed to the panel under that name; see
  Pushing data.

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
- `sources.http` — `listen`, the address and port the series of charts and
  the rows of tables are pushed to (default `127.0.0.1:4049`), and
  optionally a `token`, as a secret, that every request must carry; see
  Pushing data.
- `sources.commands` — a list of commands run on a timer, each for a
  table's rows or a chart's series; see Data from a command.
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

## Tables

A `table` tile is rows, stacked from the top of its region, each with a
`height` and the `tiles` on it. A tile on a row is placed from the row's
top left corner: `x` and `width`, and optionally `y` (default 0) and
`height` (default the rest of the row). Its `tile` is a name from `tiles`
or a tile spelt out, of any kind but `table`. `gap` puts pixels between the
rows (default 0).

```yaml
tiles:
  rooms:
    kind: table
    gap: 1
    colors: {accent: "#50aaff"}
    rows:
      - height: 7
        tiles:
          - {x: 0, width: 27, tile: {kind: text, text: "ROOM", size: 4x6, align: left}}
          - {x: 29, width: 19, tile: {kind: text, text: "°C", size: 4x6, align: right}}
      - height: 10
        tiles:
          - {x: 0, width: 27, tile: {kind: text, text: "KITCHEN", size: 4x6, align: left, overflow: truncate}}
          - {x: 29, width: 19, tile: {kind: text, text: "22.8", size: 5x8, align: right}}
          - {x: 50, y: 1, width: 12, height: 8, tile: {kind: line_chart, entity: sensor.kitchen}}
```

- Text in a table usually wants `align`, names to the left and numbers to
  the right, and `overflow: truncate` where a long name must not scroll.
- A tile must fit its row, and the rows the region; `check` names the row
  and the tile that does not.
- The table's own `scheme` and `colors` apply to every tile on it, under
  each tile's own.
- A table draws nothing itself: each of its tiles is drawn in its place,
  in row order, as if it had a region there. Tiles may overlap, the later
  over the earlier.

### Rows from data

A table can take its rows from data pushed to the panel, as an external
process would after a database query: one request with the result, a row of
the table for each row of it.

```yaml
sources:
  http: {}
tiles:
  rooms:
    kind: table
    data: rooms
    gap: 1
    rows:
      - height: 7
        tiles:
          - {x: 0, width: 27, tile: {kind: text, text: "ROOM", size: 4x6, align: left}}
      - height: 10
        repeat: true
        tiles:
          - {x: 0, width: 27, tile: {kind: text, column: room, size: 4x6, align: left, overflow: truncate}}
          - {x: 29, width: 19, tile: {kind: text, column: temp, size: 5x8, align: right}}
          - {x: 50, y: 1, width: 12, height: 8, tile: {kind: line_chart, column: history}}
```

```sh
curl -X PUT http://127.0.0.1:4049/tables/rooms -d '[
  {"room": "Kitchen", "temp": 22.8, "history": [21.0, 22.1, 22.8]},
  {"room": "Office",  "temp": 23.1, "history": [19.4, 20.0, 23.1]}
]'
```

- `data` names the table's rows; they are pushed to `/tables/NAME` (see
  [pushing-data.md](pushing-data.md)), or given by a page's `data.tables`
  for a demo.
- A row with `repeat: true` is laid out once for each pushed row, top to
  bottom in their order, as many as fit the region under the rows before
  it; pushed rows past those are not shown, and with fewer the rest of the
  region stays empty. Only the last row of a table repeats. Rows without
  `repeat` are fixed, as a header is.
- On the repeat row, a `text` tile takes `column` in place of `text`, and a
  chart takes `column` in place of `entity` and `series`. A column the
  pushed row lacks draws nothing.
- A pushed row is a JSON object, column to value. A string, a number or a
  boolean is text exactly as written, so format numbers in the query
  (`ROUND`, `printf`) the way they should read; `null` is empty; an array of
  numbers is a chart's values, oldest first.

`docs/demos/tables.yaml` has two tables, the first from data.

## Pushing data

Charts and tables can draw what another program sends to the panel over
HTTP (or what the panel fetches by itself: see Data from a command): a chart with `series: NAME` what is `PUT` to `/series/NAME`, a table
with `data: NAME` the rows `PUT` to `/tables/NAME`. Both need
`sources.http`:

```yaml
sources:
  http: {listen: "0.0.0.0:4049", token: {secret: http_token}}
```

```sh
curl http://panel.local:4049/ -H "Authorization: Bearer $TOKEN"                                  # what it takes
curl -X PUT -d '[412, 398, 455, 620]' -H "Authorization: Bearer $TOKEN" http://panel.local:4049/series/power
curl -X PUT -d '[{"room": "Kitchen", "temp": 22.8}]' -H "Authorization: Bearer $TOKEN" http://panel.local:4049/tables/rooms
```

- `listen` defaults to `127.0.0.1:4049`, this machine only; `0.0.0.0:4049`
  takes requests from the network.
- `token` is optional. With it, every request must carry the secret as a
  bearer token, and gets a 401 without. It goes over plain HTTP, so it
  keeps out what merely reaches the port, not what can read the traffic.
  Without it, anything that reaches the port can change what shows.
- What is pushed is kept in memory: a reload of the config keeps it, and
  it is gone when the program restarts.
- The endpoint has its own thread and answers each request on another, so
  a slow or stuck client never delays a frame.

[pushing-data.md](pushing-data.md) is the whole of it, written for the
program on the other end: every request and answer, the shape of the data,
and an example that pushes a query's results. `docs/demos/http.yaml` is a
config that takes pushed data and `tools/push_example.py` a program that
sends it.

## Data from a command

The panel can fetch its own data: a command in `sources.commands` is run
when the panel starts and then on a timer, and the JSON it prints becomes a
table's rows or a chart's series, exactly as if it had been pushed. A
database is read this way through its own command-line client, so none is
built into the program.

```yaml
sources:
  commands:
    - table: rooms
      every: 60
      run: [sqlite3, -json, house.db, "SELECT room, printf('%.1f', temp) AS temp FROM rooms ORDER BY room LIMIT 5"]
    - series: power
      every: 30
      run: [duckdb, -json, -readonly, metrics.duckdb, "SELECT watts FROM power ORDER BY at DESC LIMIT 120"]
```

- `run` is the program and its arguments, each its own item; no shell
  reads them, so a query needs no escaping beyond YAML's. For a pipeline
  or a redirect, run a shell: `[sh, -c, "…"]`. A long query reads best as
  a YAML block (`- |`); `docs/demos/commands.yaml` has some. A program that
  does not print JSON wants a script around it: `docs/demos/uptime.yaml` runs
  `docs/demos/uptime.sh`, which turns what `uptime` says into rows and a series.
- A series is replaced by what the command prints, so a history of
  something that only has a present (the load, a temperature) is the
  script's to keep: `docs/demos/uptime.sh history` does, in a file.
- `table: NAME` fills the table whose `data` is that name; `series: NAME`
  is the series of that name, for a chart. One or the other.
- `every` is the seconds from the end of one run to the start of the next
  (default 60). `timeout` is how long a run may take (default 30); it is
  stopped after that.
- `env` adds secrets to its environment: `env: {PGPASSWORD: {secret:
  pg_password}}`. It has the panel's own environment besides.
- It runs in the config file's directory, so `house.db` is beside the
  config, and with the rights of the panel itself: the config decides what
  the machine runs, so keep it writable only by you.
- A run that fails (a non-zero exit, a timeout, output that is not JSON)
  is logged once, with what the command said, and what was there stays on
  the panel. Each command has its own thread: a slow query never delays a
  frame or another command.
- A table or a chart may be fed by a command without `sources.http`. With
  both, whichever wrote last shows.
- `check` does not run the commands: run one by hand to see what it
  prints.

### What a command prints

For a **table**, a JSON array of objects, a row each, column to value:
what `sqlite3 -json` and `duckdb -json` print for any query.

```json
[{"room": "Kitchen", "temp": "22.8"}, {"room": "Office", "temp": "23.1"}]
```

- Text shows as printed, so round and format in the query. Rows show in
  the order printed, as many as fit the table; `ORDER BY` and `LIMIT`.
- Nothing printed is no rows, as `sqlite3 -json` does for an empty result.
- A column a chart on the row draws (`column:` on a `line_chart`, say) is
  a JSON array of numbers. Where the client prints an array as text, as
  SQLite does for `json_group_array(…)` and Postgres for an `int[]`, it is
  read as numbers: `"[1,2,3]"` and `"{1,2,3}"` both work.
- A bullet chart's `column` and `target_column` are one number each.

For a **series**, the numbers, oldest first: a JSON array of them, or rows
with the numbers in a column, the only one or the one named by `column:`
on the command. One number is a series of one, which is what a
`bullet_chart` shows.

```yaml
    - series: orders
      column: n
      run: [sqlite3, -json, shop.db, "SELECT hour, count(*) AS n FROM orders GROUP BY hour ORDER BY hour"]
```

### The three databases

```yaml
sources:
  commands:
    # SQLite: -json prints rows as objects. -readonly keeps the panel from
    # ever writing.
    - table: rooms
      run: [sqlite3, -json, -readonly, house.db, "SELECT room, temp FROM rooms"]
    # DuckDB: the same flags. It also reads CSV and Parquet files, and
    # other databases, in the query itself.
    - table: sales
      run: [duckdb, -json, -readonly, shop.duckdb, "SELECT region, round(sum(amount)) AS total FROM sales GROUP BY region ORDER BY total DESC LIMIT 5"]
    # Postgres: psql has no JSON output, so the query aggregates its rows
    # to JSON; -At prints just that. The password comes from a secret.
    - table: services
      env: {PGPASSWORD: {secret: pg_password}}
      run:
        - psql
        - -At
        - "host=db.local dbname=metrics user=panel"
        - -c
        - |
          SELECT coalesce(json_agg(t), '[]') FROM (
            SELECT name, round(avg_ms) AS ms FROM services ORDER BY avg_ms DESC LIMIT 5
          ) t
```

The client must be installed on the machine the panel runs on
(`apt install sqlite3`, `postgresql-client`; DuckDB's is a single
download). Anything else that prints JSON works the same way: `curl` and
`jq` against an API, a script of your own.

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
- `tables: {rooms: [{room: Kitchen, temp: 22.8}]}` — the rows of the table
  whose `data` is that name, as they would be pushed.
- `cover: N` — the Nth newest cover in the art cache, playing. With
  `keep_originals`, only covers with an original count.
- `picture: N` — the Nth picture, for a picture tile.

A page naming a cover or picture that is not there is left out, with a note.

The demos are in `docs/demos`. `demo.yaml` is the demo built this way: the
date alone touring the tiles,
the clock joining, every kind of sky, the print progress filling, the
laundry temperature in its place, the water leak alert, the whole dashboard
plain, a cover in the hub, the dashboard over each cover, the covers alone,
the pictures full screen, and the dashboard over them. Its timings, labels
and order are all in the file. Its `target` is the WLED-AP address; point
it at a board with `--target`:

```sh
target/release/panel-ddp run --config docs/demos/demo.yaml --target wled.local          # loops
target/release/panel-ddp run --config docs/demos/demo.yaml --target wled.local --once   # one pass
```

`text.yaml` is the text demo: every size of the `text` tile, each line in
its own size and saying which, then lines too wide for the panel,
scrolling. It needs no data.

The charts and tables each have a demo of their own, dashboards on made-up
series beside text of different sizes: `line-charts.yaml` (one reading
with its day under it, three readings each beside its chart, the clock and
the date over a reading, text over a chart the size of the panel),
`area-charts.yaml` (one with a gradient under a reading and three beside
theirs), `bar-charts.yaml` (the same way), `tables.yaml` (two tables, one
from data) and `bullet-charts.yaml` (one under a reading and a table of
them). `http.yaml`, `commands.yaml` and `uptime.yaml` are the demos of
data that is pushed or fetched by a command.
[demos/README.md](demos/README.md) shows each with its picture.

`art_file`, a top-level setting, keeps a JPEG at the album cover on show,
the original as downloaded, black when no cover is on, and removes it when
the run ends; `docs/demos/demo.yaml` sets it to `../../demo-art.jpg`, at
the top of the repository. `art_open: true` runs `open` on it after each change for macOS Preview. `tools/DemoArtViewer` is
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
target/release/panel-ddp run --config docs/demos/demo.yaml --target <board>   # the demo; --once for a single pass
target/release/panel-ddp preview --out preview.png  # the first page from sample data, mask applied
target/release/panel-ddp preview --page home        # that page instead of the first
target/release/panel-ddp preview --commands         # run sources.commands once and draw what they print, not made-up values
target/release/panel-ddp preview --push "sh docs/demos/http-push.sh"   # serve sources.http while that runs and draw what it pushed
target/release/panel-ddp preview --all --out p.png  # every page, a still picture each, into p-NAME.png
target/release/panel-ddp preview --all-in-one --out p.gif  # every page in the file, in order, each for its time, as one animation
target/release/panel-ddp preview --out preview.gif  # the first page animated, for its time; --seconds N for longer or shorter
target/release/panel-ddp preview --out preview.apng # the same as an animated PNG, in full colour (GIF has 256 a frame)
target/release/panel-ddp preview --weather-code 95  # check an icon (add 1000 for night)
target/release/panel-ddp preview --alert            # the alert view
target/release/panel-ddp test <board-ip>            # colour bars, ramp, counter, bouncing dot; --size 128x64 for another panel
target/release/panel-ddp render --config docs/demos/demo.yaml --out /tmp/r   # every frame's hash, no network
target/release/panel-ddp check dashboard.yaml docs/demos/demo.yaml   # load each, say what it holds or what is wrong
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
target/release/panel-ddp run --config docs/demos/demo.yaml --target <board> --once && \
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
