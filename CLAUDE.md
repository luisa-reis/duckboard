# CLAUDE.md

Context for Claude Code sessions working on panel-ddp.

## What it is

- A standalone Rust program that draws a dashboard for a 64×64 WLED matrix and
  streams it to the board over DDP (UDP 4048). Nothing runs on the board; WLED falls back to its presets a
  couple of seconds after the stream stops.
- It runs either on a Raspberry Pi near the panel (cross-compiled, as a
  systemd service) or in the background on a laptop (launchd agent on macOS).
  It needs only network access to the board and its data sources.
- `gaps` (default `2d-gaps.json`, relative to the config file) is a copy of
  the board's WLED gap file (1 = lit), used only by `preview` to grey out
  hidden pixels; without it the preview shows the whole panel. The real
  `2d-gaps.json` is git-ignored; `2d-gaps.example.json` (four corners
  hidden) is the committed sample. The gap file does not shape the
  layout.
- Config files are read by a loader into `Model` (`src/model.rs`): named
  pages, each a list of layers (an area, a tile, its palette) in drawing
  order; playlists of pages; and a schedule whose first matching rule picks
  the playlist at each page's end (none matching: nothing is sent, so the
  board falls back to its presets). `src/pages.rs` plays it. The
  drawing code and the sources only see the model. Today's TOML/JSON files
  go through `src/legacy.rs`, where the five regions of `[regions]` (x, y,
  width, height, `z`) and the background become layers (the hub an `art`
  tile, the background an `art` or `picture` tile with an alpha); `src/config.rs`
  holds the building blocks any format shares. Layers have no background,
  so overlaps composite. Drawing code must take its size from the area it
  is given, never a constant. Album art and pictures are decoded once per
  size they show at (`picture::Scaled`, `Model::art_sizes`).

## Layout

- `src/main.rs` — CLI (`run`, `frame`, `preview`, `test`, `spotify-login`).
- `src/model.rs` — the resolved configuration everything works from.
- `src/format.rs` — the YAML configuration file (`.yaml`/`.yml`): named
  schemes, layouts, tiles, pages, playlists and a schedule, resolved into
  the model with errors that name the page and region.
- `src/legacy.rs` — today's TOML/JSON files, read into the model.
- `src/config.rs` — building blocks shared by any format (sources, alerts,
  tile kinds, page data).
- `src/ddp.rs` — the sender (an unconnected UDP socket, on purpose).
- `src/dashboard.rs` — draws a page's layers in order; `src/tiles.rs`
  draws any tile kind into a given area, `art.rs` the art and picture
  kinds; `pages.rs` times the pages.
- `src/mask.rs` — the gap file, for previews only.
- `src/ha.rs`, `spotify.rs`, `weather.rs`, `data.rs` — data sources, each on
  its own thread keeping the last good reading.
- `demo.json` — the demo, a config with `pages`.
- `tools/DemoArtViewer/` — a Processing sketch for the demo art.

## Building

The crate pins its toolchain in `rust-toolchain.toml`; rustup installs it on
the first `cargo` run here.

```sh
cargo build --release
cargo clippy
target/release/panel-ddp preview --out preview.png   # check a change without the board
target/release/panel-ddp render --config demo.json --out /tmp/before   # frame hashes; diff before/after a refactor
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

- `docs/dashboard.md` owns the documentation, `docs/ddp.md` the protocol and
  what WLED does with it; the README only points at them.
  Update the docs alongside behaviour or config changes, and
  `dashboard.example.toml` alongside new settings.
- `dashboard.toml` (and `dashboard-*.toml`) hold the Home Assistant token,
  `spotify-token.json` the Spotify refresh token; both are git-ignored, as
  are `art-cache/`, `frame/`, `demo-art.jpg` and `2d-gaps.json`. Never commit
  them.
- Stopping: Ctrl-C/SIGINT ends a run cleanly; services should send SIGINT
  (`KillSignal=SIGINT` in the systemd unit).
- The repo uses jujutsu (colocated with git).
