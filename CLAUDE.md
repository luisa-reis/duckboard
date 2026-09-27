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
- The screen is split into five regions (four tiles and the hub), set in
  `[regions]` (`Regions` in `src/config.rs`) with x, y, width, height and a
  `z`; they are drawn in ascending `z`, ties in `Slot` order with the hub
  last. Regions have no background, so overlaps composite. Drawing code
  must take its size from the region it is given, never a constant; hub art
  is decoded at the hub region's size (`ArtCache::hub`).

## Layout

- `src/main.rs` — CLI (`run`, `frame`, `preview`, `test`, `spotify-login`).
- `src/config.rs` — TOML/JSON config, regions, defaults, path resolution.
- `src/ddp.rs` — the sender (an unconnected UDP socket, on purpose).
- `src/dashboard.rs` — draws the regions in `z` order; `src/tiles.rs` and
  `hub.rs` draw into a given area; `pages.rs` loops layouts.
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
