#!/bin/sh
# Draws the pictures README.md here shows, for every demo and for
# dashboard.example.yaml, with the program's own `preview`. From the top of the repository, after
# `cargo build --release`:
#
#   sh docs/demos/render.sh
#
# The demos are copied to an empty directory first, so that no gap file,
# cover or picture of this machine's is in the pictures. commands.yaml and
# uptime.yaml are drawn with what their commands print now (`--commands`),
# http.yaml with what http-push.sh pushes to it (`--push`).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=${PANEL_DDP:-$here/../../target/release/panel-ddp}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$here"/*.yaml "$here"/uptime.sh "$here"/http-push.sh "$here"/../../dashboard.example.yaml "$work"

# An animated PNG of one pass through a config's pages, named .png: GitHub
# serves that as a PNG, which a browser animates, and does not know .apng.
# `preview` goes by the name, so it is drawn as .apng and then renamed.
animate() {
  name=$1
  shift
  "$bin" preview --config "$work/$name.yaml" --all-in-one --out "$work/$name.apng" "$@"
  mv "$work/$name.apng" "$here/$(echo "$name" | tr . -).png"
}

# The tours go up to the album art, which is not there; the example is on
# sample data.
for name in demo demo-128x64 text fonts line-charts area-charts bar-charts bullet-charts tables dashboard.example; do
  animate "$name"
done
animate commands --commands
animate http --push "sh $work/http-push.sh"

# One page that does not move: a still.
"$bin" preview --config "$work/uptime.yaml" --commands --out "$here/uptime.png"
