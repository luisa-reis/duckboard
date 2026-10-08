#!/usr/bin/env bash
# Set which side of a WLED matrix panel is up, on a running board, so the
# picture stays upright whichever way the panel is mounted.
#
# Usage: ./orient.sh [options] [top|right|bottom|left] [host]
#   [side]                 The panel's side that is up. Without one the script
#                          only reports the board's current orientation.
#   [host]                 Board IP or hostname (default: 4.3.2.1, the WLED-AP address)
#   -n, --no-reboot        Save the setting without rebooting (it applies at the next boot)
#   -P, --pin <pin>        Settings PIN, if the board has one
#   -h, --help             Show this help
#
# The sides are the panel's own, named as WLED maps it with no flags set:
# LED 0 in the top-left corner and rows running left to right. "top" is
# that orientation; "left" means the panel is turned a quarter clockwise so
# its left side is up, "bottom" a half turn, "right" a quarter anticlockwise.
# To find the panel's top, set "top" and see which side the picture's top
# is on.
#
# The orientation is WLED's 2D panel layout, the b/r/v flags of
# hw.led.matrix.panels[0], and its w and h: a rotation of the logical map
# over the physical panel. Effects and anything streamed (DDP, E1.31, ...)
# draw in the logical map, so they come out upright; a gap file
# (2d-gaps.json) is in the logical map too.
#
# Any panel size works. On a panel that is not square, a quarter turn
# swaps the logical map's width and height (a 128x64 panel becomes 64x128),
# so whatever streams to it and any gap file must then be that shape.
#
# The script handles a matrix of one panel only and refuses one it does not
# recognise (several panels, or flags that are none of the four
# orientations) rather than guess. Other panel settings (position,
# serpentine wiring) are kept.

set -euo pipefail

DEFAULT_HOST=4.3.2.1
HOST=""
WANT=""
REBOOT=1
PIN=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--no-reboot) REBOOT=0; shift ;;
    -P|--pin)       PIN="$2"; shift 2 ;;
    -h|--help)      sed -n '2,33{s/^# \{0,1\}//p;}' "$0"; exit 0 ;;
    -*) echo "Unknown option: $1 (see --help)" >&2; exit 1 ;;
    top|right|bottom|left)
        [[ -z "$WANT" ]] || { echo "Two sides given: $WANT and $1" >&2; exit 1; }
        WANT="$1"; shift ;;
    *)  [[ -z "$HOST" ]] || { echo "Unexpected argument: $1 (sides are top, right, bottom, left; the host is $HOST)" >&2; exit 1; }
        HOST="$1"; shift ;;
  esac
done
HOST=${HOST:-$DEFAULT_HOST}
BASE="http://$HOST"
CURL=(curl -sS --connect-timeout 5 --max-time 30)

die() { echo "ERROR: $*" >&2; exit 1; }

# The four orientations as WLED panel flags "b r v" (bottomStart, rightStart,
# vertical), by the side that is up. Derived from setUpMatrix() in
# wled00/FX_2Dfcn.cpp: the flags place LED 0 and the direction of its row
# in the logical map. The panel's w and h are the logical size; with v set
# the physical rows run along h. The flags are the same at any size.
#   top     as wired: LED 0 top left, rows left to right
#   left    turned a quarter clockwise: LED 0 top right, rows downwards
#   bottom  turned a half turn: LED 0 bottom right, rows right to left
#   right   turned a quarter anticlockwise: LED 0 bottom left, rows upwards
flags_for() {
  case "$1" in
    top)    echo "false false false" ;;
    left)   echo "false true true" ;;
    bottom) echo "true true false" ;;
    right)  echo "true false true" ;;
  esac
}

# side_for <b> <r> <v>: the side that is up for a flag triple, or nothing
side_for() {
  for o in top right bottom left; do
    [[ "$(flags_for "$o")" == "$1 $2 $3" ]] && { echo "$o"; return; }
  done
  return 0
}

# matrix_panel: the live panel as "b r v width height x y s", where width
# and height are the panel's own (its rows' length, then their number),
# after checking the matrix is one panel; prints the block and fails
# otherwise.
matrix_panel() {
  LIVE_JSON="$1" python3 - <<'PY'
import json, os, sys
m = json.loads(os.environ["LIVE_JSON"]).get("hw", {}).get("led", {}).get("matrix")
ok = (isinstance(m, dict) and m.get("mpc") == 1 and isinstance(m.get("panels"), list)
      and len(m["panels"]) == 1)
p = m["panels"][0] if ok else {}
w, h = p.get("w"), p.get("h")
x, y = p.get("x", 0), p.get("y", 0)
ok = ok and all(isinstance(n, int) for n in (w, h, x, y)) and w > 0 and h > 0 and x >= 0 and y >= 0
if not ok:
    print("hw.led.matrix on the board:", json.dumps(m), file=sys.stderr)
    sys.exit(1)
if p.get("v", False):
    w, h = h, w
flag = lambda k: str(bool(p.get(k, False))).lower()
print(flag("b"), flag("r"), flag("v"), w, h, x, y, flag("s"))
PY
}

# logical_size <v>: the logical map's "width height" with vertical flag v
logical_size() {
  if [[ "$1" == true ]]; then echo "$PH $PW"; else echo "$PW $PH"; fi
}

# Read the live config and name its orientation.
read_matrix() {
  LIVE=$("${CURL[@]}" "$BASE/json/cfg") || die "cannot read /json/cfg from $HOST"
  MATRIX=$(matrix_panel "$LIVE") || die "unrecognised matrix layout on the board (above); this script handles a single panel"
  read -r B R V PW PH PX PY S <<<"$MATRIX"
  CURRENT=$(side_for "$B" "$R" "$V")
  [[ -n "$CURRENT" ]] || die "the board's panel flags (b=$B r=$R v=$V) are none of the four orientations"
  read -r LW LH <<<"$(logical_size "$V")"
}

read_matrix
echo "On $HOST the panel's $CURRENT side is up (${PW}x${PH} panel, logical map ${LW}x${LH})"
[[ -n "$WANT" ]] || exit 0
if [[ "$WANT" == "$CURRENT" ]]; then
  echo "Already $WANT up; nothing to do."
  exit 0
fi

OLD_SIZE="${LW}x${LH}"
read -r B R V <<<"$(flags_for "$WANT")"
read -r LW LH <<<"$(logical_size "$V")"
BODY="{\"hw\":{\"led\":{\"matrix\":{\"mpc\":1,\"panels\":[{\"x\":$PX,\"y\":$PY,\"w\":$LW,\"h\":$LH,\"b\":$B,\"r\":$R,\"v\":$V,\"s\":$S}]}}}}"
[[ -z "$PIN" ]] || BODY="{\"pin\":\"$PIN\",${BODY#\{}"
echo "Setting $WANT up (b=$B r=$R v=$V, logical map ${LW}x${LH}) ..."
if [[ "${LW}x${LH}" != "$OLD_SIZE" ]]; then
  echo "Note: the logical map is now ${LW}x${LH}; whatever streams to the board, and any gap file, must be that shape."
fi
RESP=$("${CURL[@]}" -H 'Content-Type: application/json' --data "$BODY" "$BASE/json/cfg") || die "POST /json/cfg failed"
[[ "$RESP" == *'"success":true'* ]] || die "unexpected response from /json/cfg: $RESP (settings PIN? pass -P <pin>)"

if [[ $REBOOT -eq 0 ]]; then
  echo "Saved; the layout applies at the next reboot (-n)."
  exit 0
fi
echo "Rebooting ..."
"${CURL[@]}" -o /dev/null -H 'Content-Type: application/json' --data '{"rb":true}' "$BASE/json/state" || die "reboot request failed"
for _ in $(seq 1 30); do
  sleep 2
  curl -s --connect-timeout 2 --max-time 5 -o /dev/null "$BASE/json/info" && break
done
read_matrix
[[ "$CURRENT" == "$WANT" ]] || die "after the reboot the board reports $CURRENT up, not $WANT"
echo "Done: $WANT up."
