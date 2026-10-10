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

# One pass through each demo's pages (demo.yaml: up to the album art, which
# is not there).
for name in demo text line-charts area-charts bar-charts bullet-charts tables; do
  "$bin" preview --config "$work/$name.yaml" --all-in-one --out "$here/$name.gif"
done

"$bin" preview --config "$work/commands.yaml" --commands --all-in-one --out "$here/commands.gif"
"$bin" preview --config "$work/uptime.yaml" --commands --out "$here/uptime.png"
"$bin" preview --config "$work/http.yaml" --push "sh $work/http-push.sh" --all-in-one --out "$here/http.gif"

# Two that are too large as one animation of everything (jujutsu takes no
# new file over 1 MiB): the first half minute of the 128×64 tour, and the
# example's pages as a still each, on sample data.
"$bin" preview --config "$work/demo-128x64.yaml" --seconds 30 --out "$here/demo-128x64.gif"
"$bin" preview --config "$work/dashboard.example.yaml" --all --out "$here/dashboard-example.png"
