# CLAUDE.md

Context for Claude Code sessions working on panel-ddp.

## What it is

- A standalone Rust program that draws a dashboard for a WLED matrix (64×64
  unless the config's `width` and `height` say otherwise)
  and streams it to the board over DDP (UDP 4048). Nothing runs on the
  board; WLED falls back to its presets a couple of seconds after the stream
  stops.
- It runs either on a Raspberry Pi near the panel (cross-compiled, as a
  systemd service) or in the background on a laptop (launchd agent on
  macOS). It needs only network access to the board and its data sources.
- One YAML file drives it (`dashboard.yaml` by default; `docs/demos/demo.yaml`
  is the demo, `dashboard.example.yaml` the starting point): named colour schemes,
  layouts (regions with x, y, width, height, `z`), tiles, pages, playlists
  and a schedule. It is validated by `panel-ddp.schema.json` and
  `panel-ddp check`, and a running `run` reloads it when it changes.
- `src/format.rs` resolves the file into `Model` (`src/model.rs`): named
  pages, each a list of layers (an area, a tile, its palette) in drawing
  order; playlists of pages; and a schedule whose first matching rule picks
  the playlist at each page's end (none matching: nothing is sent, so the
  board falls back to its presets). `src/pages.rs` plays it. The drawing
  code and the sources only see the model. Layers have no background, so
  overlaps composite. Drawing code must take its size from the area it is
  given, never a constant. Album art and pictures are decoded once per size
  they show at (`picture::Scaled`, `Model::art_sizes`).
- A running `run` reloads the model when `Model::files` change and the new
  one loads (`Live::reload` in `src/main.rs`); sources restart only when
  their settings change.
- `gaps` (default `2d-gaps.json`, relative to the config file) is a copy of
  the board's WLED gap file (1 = lit), used only by `preview` to grey out
  hidden pixels; without it the preview shows the whole panel. The real
  `2d-gaps.json` is git-ignored; `2d-gaps.example.json` (four corners
  hidden) is the committed sample. The gap file does not shape the layout.

## Layout

- `src/main.rs` — CLI (`run`, `preview`, `render`, `test`, `spotify-login`,
  `check`, `schema`, `migrate`).
- `src/format.rs` — the YAML configuration file, resolved into the model
  with errors that name the page and region.
- `src/model.rs` — the resolved configuration everything works from.
- `src/config.rs` — building blocks the file is made of (sources, alerts,
  tile kinds, page data, schedule rules).
- `src/secrets.rs` — `{secret: name}` references.
- `panel-ddp.schema.json` — the file's JSON Schema, generated from the
  format's types (their doc comments are its descriptions). After changing
  them: `target/release/panel-ddp schema > panel-ddp.schema.json`; a test
  fails until it is current.
- `src/legacy.rs`, `src/migrate.rs` — the older TOML/JSON configs, read only
  by `panel-ddp migrate`, which rewrites one as YAML (token to the secrets
  file) and checks with `same_drawing` that it draws the same.
- `src/ddp.rs` — the sender (an unconnected UDP socket, on purpose).
- `src/dashboard.rs` — draws a page's layers in order; `src/tiles.rs`
  draws any tile kind into a given area, `art.rs` the art and picture
  kinds; `pages.rs` plays the playlists.
- `src/mask.rs` — the gap file, for previews only.
- `src/ha.rs`, `spotify.rs`, `weather.rs`, `data.rs` — data sources, each on
  its own thread keeping the last good reading.
- `src/http.rs` — the HTTP endpoint chart series and table rows are pushed
  to (`sources.http`), on its own thread, a thread per request.
- `src/command.rs` — commands run on a timer for the same series and rows
  (`sources.commands`): how a database is read, through its CLI, with
  nothing linked in. `docs/demos/commands.yaml` is the working example, and
  `docs/demos/uptime.yaml` with `docs/demos/uptime.sh` the one of a script.
- `docs/demos/` — the demos, a config for each tile kind, chart kind and
  data source, and its `README.md`, which shows a picture of each;
  `render.sh` there draws the pictures. A config's paths are relative to
  it, so these name the gap file, art cache, pictures and `art_file` as
  `../../…`: all of those stay at the top of the repository.
- `tools/DemoArtViewer/` — a Processing sketch for the demo art.

## Building

The crate pins its toolchain in `rust-toolchain.toml`; rustup installs it on
the first `cargo` run here.

```sh
cargo build --release
cargo clippy --all-targets
cargo test
target/release/panel-ddp check dashboard.example.yaml docs/demos/demo.yaml
target/release/panel-ddp preview --out preview.png   # check a change without the board (--page NAME, --all, --all-in-one, --commands, --push CMD)
target/release/panel-ddp render --config docs/demos/demo.yaml --out /tmp/before   # frame hashes; diff before/after a refactor
```

Cross-compiling for a Raspberry Pi (64-bit OS) from the Mac. `ring` needs a
C cross-compiler, which cargo-zigbuild supplies through Zig:

```sh
brew install zig && cargo install --locked cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu     # run inside the repo so it lands on the pinned toolchain
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.31
# -> target/aarch64-unknown-linux-gnu/release/panel-ddp
```

Use `armv7-unknown-linux-gnueabihf` for 32-bit Raspberry Pi OS; `cross build`
(Docker) is the alternative to zigbuild. Copying to the Pi, the systemd unit
and the launchd plist are in `docs/dashboard.md` under "Installing".

## Conventions

- `docs/dashboard.md` owns the documentation, `docs/charts.md` the text,
  chart and table tiles by example, `docs/designing-dashboards.md` how to design a page
  (the check-and-preview loop, pixel budgets, layouts to copy; written for
  an agent asked for a dashboard), `docs/pushing-data.md` the HTTP
  endpoint (written to stand alone, for a program or an agent in another
  repository that pushes data here) and `docs/ddp.md` the protocol and what
  WLED does with it; the README introduces the project and points at them.
  `docs/demos/http.yaml`, `docs/demos/http-push.sh` and
  `tools/push_example.py` are the working example of pushed data: keep
  them running against each other.
- The pictures in `docs/demos`, some of them in the README too, are
  `preview` output: `sh docs/demos/render.sh` draws them all again, from
  copies of the configs in an empty directory so that no gap file, cover
  or picture is in them, `dashboard.example.yaml`'s pages too. Draw them
  again when a demo or its tiles' drawing
  changes; jujutsu refuses a new file over 1 MiB. Update the docs
  alongside behaviour or config changes, the schema alongside the format's
  types, and `dashboard.example.yaml` alongside new settings.
- When editing a config, run `panel-ddp check` on it; refactors of the
  drawing should leave `render` output of `docs/demos/demo.yaml` unchanged.
- In configs secrets are references, `token: {secret: name}`, resolved from
  `PANEL_DDP_SECRET_<NAME>` or `secrets.yaml` beside the config
  (`secrets.example.yaml` shows the shape). Never read or print
  `secrets.yaml`; a config never holds a secret itself.
- Git-ignored, never committed: `dashboard.yaml` (and `dashboard-*.yaml`),
  `secrets.yaml`, `spotify-token.json` (the Spotify refresh token),
  `art-cache/`, `frame/`, `demo-art.jpg`, `2d-gaps.json`, and older
  `dashboard*.toml` files, which hold a token.
- Stopping: Ctrl-C/SIGINT ends a run cleanly; services should send SIGINT
  (`KillSignal=SIGINT` in the systemd unit).
- The repo uses jujutsu (colocated with git).
