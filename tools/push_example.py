#!/usr/bin/env python3
"""Pushes made-up data to a panel running docs/demos/http.yaml.

A reference for pushing from another program: the standard library only,
one request per series or table. See docs/pushing-data.md.

    python3 tools/push_example.py [--url http://127.0.0.1:4049] [--every SECONDS]

The token, when the panel asks for one, comes from DUCKBOARD_TOKEN.
"""

import argparse
import json
import math
import os
import random
import time
import urllib.error
import urllib.request


def send(base, method, path, body=None):
    """One request; the panel's answer as text. Raises on a refusal."""
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base.rstrip("/") + path, data=data, method=method)
    token = os.environ.get("DUCKBOARD_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.read().decode().strip()
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{method} {path}: {e.code} {e.read().decode().strip()}")
    except urllib.error.URLError as e:
        raise SystemExit(f"{method} {path}: {e.reason} (is the panel running, with sources.http?)")


def wave(n, base, swing, speed, phase=0.0):
    return [round(base + swing * math.sin(i / speed + phase) + random.uniform(-swing, swing) / 6, 1) for i in range(n)]


def push(base):
    # A chart's series: all of its values at once, oldest first.
    print("power:", send(base, "PUT", "/series/power", wave(60, 600, 250, 6.0)))
    print("solar:", send(base, "PUT", "/series/solar", [max(0.0, v) for v in wave(60, 1.2, 2.2, 9.0, -1.5)]))
    print("rain:", send(base, "PUT", "/series/rain", [0, 2.5, 7, 1, 0, 4.5, 12, 3, 0, 0, 6, 9, 2, 0, 1.5]))
    # A table's rows: what a query would return, a row each, column to value.
    # Text is shown as sent, so numbers are rounded here.
    rows = [
        {
            "name": name,
            "ms": round(ms),
            "latency": wave(24, ms, ms / 4, 3.0, i),
            "requests": [max(0, round(v)) for v in wave(12, 40, 30, 2.0, i)],
        }
        for i, (name, ms) in enumerate([("api", 42), ("auth", 18), ("search", 131), ("billing", 77), ("reports", 260)])
    ]
    print("services:", send(base, "PUT", "/tables/services", rows))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:4049", help="the panel's sources.http address")
    parser.add_argument("--every", type=float, help="push again every this many seconds")
    args = parser.parse_args()
    print(send(args.url, "GET", "/"))  # what the panel takes
    while True:
        push(args.url)
        if not args.every:
            break
        time.sleep(args.every)


if __name__ == "__main__":
    main()
