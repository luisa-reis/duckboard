#!/bin/sh
# What `uptime` says, as the JSON a `sources.commands` entry reads;
# uptime.yaml runs it. On macOS and Linux.
#
#   sh uptime.sh loads     the three load averages, a table's rows
#   sh uptime.sh up        how long the machine has been up, one row
#   sh uptime.sh history   the one-minute load at each run, a series
set -eu

# How many runs `history` keeps, and where: a command's series is replaced
# by what it prints, so the past is the script's to remember.
KEEP=60
HISTORY="${TMPDIR:-/tmp}/duckboard-uptime-history"

line=$(LC_ALL=C uptime)
# "load averages: 4.36 3.73 2.34" on macOS, "load average: 0.52, 0.58, 0.59"
# on Linux.
loads=$(printf '%s\n' "$line" | sed -E 's/.*load averages?: *//; s/,//g')
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)

case "${1:-loads}" in
  loads)
    # `percent` is the load against the cores, for a bullet chart.
    printf '%s\n' "$loads" | awk -v cores="$cores" '{
      split("1 MIN,5 MIN,15 MIN", name, ",")
      printf "["
      for (i = 1; i <= 3; i++) {
        percent = $i * 100 / cores
        printf "%s{\"name\": \"%s\", \"load\": \"%.2f\", \"percent\": %d}",
          (i > 1 ? ", " : ""), name[i], $i, (percent > 100 ? 100 : percent)
      }
      print "]"
    }'
    ;;
  up)
    # "up 7 days,  2:38, 10 users" becomes "7d 2:38".
    up=$(printf '%s\n' "$line" | sed -E '
      s/.* up +//; s/, +[0-9]+ users?.*//
      s/ days?,?/d/; s/ hrs?/h/; s/ mins?/m/; s/ secs?/s/; s/  +/ /g')
    printf '[{"up": "%s"}]\n' "$up"
    ;;
  history)
    printf '%s\n' "${loads%% *}" >> "$HISTORY"
    tail -n "$KEEP" "$HISTORY" > "$HISTORY.new" && mv "$HISTORY.new" "$HISTORY"
    awk 'BEGIN { printf "[" } { printf "%s%s", (NR > 1 ? ", " : ""), $1 } END { print "]" }' "$HISTORY"
    ;;
  *)
    echo "usage: $0 [loads|up|history]" >&2
    exit 2
    ;;
esac
