# panel-ddp

Streams a dashboard (clock, date, weather, Home Assistant sensors, album art,
picture frames) over DDP to a 64×64 WLED matrix.

```sh
cargo build --release
cp dashboard.example.toml dashboard.toml    # set target, [weather] and the tiles
target/release/panel-ddp preview            # one frame as a PNG, mask applied
target/release/panel-ddp run                # stream until Ctrl-C
```

Configuration, installing on a Raspberry Pi or as a background service,
Spotify setup, pages and the demo are all in
[docs/dashboard.md](docs/dashboard.md).
