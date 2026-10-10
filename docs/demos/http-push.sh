#!/bin/sh
# Pushes made-up data to a panel running http.yaml: each `cat` is the
# program that has the data, and `curl` sends what it prints.
#
#   sh docs/demos/http-push.sh [http://127.0.0.1:4049]
#
# tools/push_example.py does the same from Python, again and again with
# new values; docs/pushing-data.md has every request and answer.
set -eu

url=${1:-http://127.0.0.1:4049}

# PUT what is on standard input to a path, replacing what was there. The
# token, when the panel asks for one, comes from DUCKBOARD_TOKEN.
put() {
  printf '%s: ' "$1"
  curl -sS --fail-with-body -X PUT --data-binary @- \
    ${DUCKBOARD_TOKEN:+-H "Authorization: Bearer $DUCKBOARD_TOKEN"} "$url$1"
}

# A chart's series: all of its values at once, oldest first.
cat <<JSON | put /series/power
[420, 552, 595, 579, 604, 688, 736, 676, 563, 502, 501, 493, 521, 521, 585,
 693, 738, 677, 970, 946, 957, 904, 440, 461, 558, 680, 719, 663, 610, 623]
JSON

cat <<JSON | put /series/solar
[0, 0, 0.1, 0.1, 0.2, 0.3, 0.6, 0.8, 0.9, 0.8, 1.1, 1.6, 2.0, 2.1, 1.8, 1.6,
 1.8, 2.5, 3.1, 3.2, 2.7, 2.2, 2.2, 2.8, 3.4, 3.6, 3.0, 2.3, 2.0, 2.3]
JSON

cat <<JSON | put /series/rain
[0, 2.5, 7, 1, 0, 4.5, 12, 3, 0, 0, 6, 9, 2, 0, 1.5]
JSON

# A table's rows: what a query would return, a row each, column to value.
# An array of numbers is the chart on that row.
cat <<JSON | put /tables/services
[
  {"name": "api",     "ms": 42,  "latency": [40, 44, 41, 47, 39, 42, 45, 42], "requests": [30, 42, 55, 61, 48, 37, 52, 66]},
  {"name": "auth",    "ms": 18,  "latency": [20, 19, 17, 18, 21, 17, 18, 18], "requests": [12, 15, 11, 18, 22, 19, 14, 16]},
  {"name": "search",  "ms": 131, "latency": [90, 120, 150, 140, 125, 160, 138, 131], "requests": [70, 64, 58, 61, 75, 82, 79, 68]},
  {"name": "billing", "ms": 77,  "latency": [80, 70, 75, 79, 83, 72, 74, 77], "requests": [5, 9, 7, 12, 8, 6, 10, 11]},
  {"name": "reports", "ms": 260, "latency": [300, 280, 240, 250, 290, 270, 255, 260], "requests": [2, 3, 1, 4, 6, 3, 2, 5]}
]
JSON
