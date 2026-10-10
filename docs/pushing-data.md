# Pushing data to the panel

Everything another program needs to put its own numbers and text on the
panel: `panel-ddp` runs a small HTTP endpoint, and charts and tables draw
what is sent to it. This page stands on its own; the tiles themselves are
in [charts.md](charts.md) and the rest of the configuration in
[dashboard.md](dashboard.md).

## In short

1. The panel's config turns the endpoint on and names what it takes: a
   chart with `series: NAME`, a table with `data: NAME`.
2. `GET /` on the endpoint lists those names, and each table's columns.
3. `PUT /series/NAME` with a JSON array of numbers draws a chart.
4. `PUT /tables/NAME` with a JSON array of objects fills a table, a row
   for each.
5. Push again whenever the data changes, and once when the panel starts:
   it keeps what it is sent in memory only.

```sh
curl http://127.0.0.1:4049/
curl -X PUT -d '[412, 398, 455, 620]' http://127.0.0.1:4049/series/power
curl -X PUT -d '[{"name": "api", "ms": 42, "latency": [40, 44, 42]}]' http://127.0.0.1:4049/tables/services
```

`charts.example.yaml` is a config that takes exactly these, and
`tools/push_example.py` a program that sends them; run the two to see it
work end to end:

```sh
target/release/panel-ddp run --config charts.example.yaml --target <board>
python3 tools/push_example.py            # in another terminal
```

## Where the endpoint is

The config file (`dashboard.yaml` unless the panel was started with
`--config`) has it under `sources.http`:

```yaml
sources:
  http: {listen: "127.0.0.1:4049", token: {secret: http_token}}
```

- `listen` is the address and port, `127.0.0.1:4049` when left out, which
  only takes requests from the same machine. A panel that takes them from
  the network says `0.0.0.0:4049`; reach it at the machine's own name or
  address, `http://panel.local:4049`.
- Without `sources.http` there is no endpoint: add it, and the running
  panel picks the change up within a second. It says `http: listening on
  …` when it is up.
- It is plain HTTP, not HTTPS.

### The token

When `sources.http` has a `token`, every request, `GET` included, must
carry it:

```
Authorization: Bearer THE-TOKEN
```

The config only names the secret. Its value is in the environment variable
`PANEL_DDP_SECRET_<NAME>` of the panel's process (`PANEL_DDP_SECRET_HTTP_TOKEN`
for `{secret: http_token}`), or in `secrets.yaml` beside the config. Whoever
runs the panel hands it to the program that pushes; do not read or print
`secrets.yaml` from a tool that logs what it does. Without a `token` in the
config, no header is needed.

## What it takes

```sh
curl http://127.0.0.1:4049/
```

```json
{
  "series": ["power", "rain", "solar"],
  "tables": {
    "services": {"latency": "numbers", "ms": "text", "name": "text", "requests": "numbers"}
  }
}
```

- `series` are the names charts draw; each takes an array of numbers.
- `tables` are the names tables draw, each with the columns its tiles
  read: `text` for a column shown as text, `numbers` for one a line, area
  or bar chart draws (an array), `number` for a bullet chart's value or
  target (one number).
- A name that is not listed is a 404: the panel only takes what its config
  draws. To add one, add the chart or the table to the config first (see
  [charts.md](charts.md)).

The same names are in the config: `series:` on a `line_chart`, `area_chart`
or `bar_chart` tile, `data:` on a `table` tile, and `column:` on the tiles
of its repeat row.

## A chart's series

| Request | Body | Does | Answers |
|---|---|---|---|
| `PUT /series/NAME` | `[1, 2.5, 3]` | replaces the series | `3 values` |
| `POST /series/NAME` | `4` or `[4, 5]` | adds to its end | `5 values` |
| `GET /series/NAME` | | | `[1.0,2.5,3.0,4.0,5.0]` |
| `DELETE /series/NAME` | | empties it | `0 values` |

- The values are oldest first; the last is the latest, where a line
  chart's dot and a bar chart's `last` colour go.
- A chart shows the whole series across its width, however many values
  there are: more than it has pixels are averaged, fewer are stretched. To
  show the last hour, send the last hour.
- A series keeps its newest 1024 values. Pushing one value at a time with
  `POST` therefore makes a rolling window by itself.
- Line and area charts scale from the lowest value to the highest. Bar
  charts stand on zero.
- A bullet chart shows one value, the latest of its series: `POST` a
  number, or `PUT` an array of one. Its scale, target and bands are in the
  config.

## A table's rows

| Request | Body | Does | Answers |
|---|---|---|---|
| `PUT /tables/NAME` | `[{…}, {…}]` | replaces the rows | `2 rows` |
| `POST /tables/NAME` | `{…}` or `[{…}]` | adds to their end | `3 rows` |
| `GET /tables/NAME` | | | `3 rows` |
| `DELETE /tables/NAME` | | empties it | `0 rows` |

A row is a JSON object, column to value:

```json
[
  {"name": "api",    "ms": 42,  "latency": [40, 44, 42, 47], "requests": [30, 41, 38]},
  {"name": "search", "ms": 131, "latency": [120, 140, 131],  "requests": [12, 9, 15]}
]
```

- A string, a number or a boolean is shown as text exactly as sent:
  `22.8333` shows as `22.8333`. Round and format in the query or the
  program (`ROUND(x, 1)`, `f"{x:.1f}"`), units included if they should
  show.
- `null` shows as nothing. A column a row lacks shows as nothing too.
- An array of numbers is a chart's values, oldest first.
- A `number` column, a bullet chart's value or target, is a number, or a
  string holding one; from an array it takes the last.
- Anything else, an object or an array holding something that is not a
  number, is refused.
- Columns the table does not read are ignored, so a query can return more
  than is shown.
- The rows show top to bottom in the order sent: sort in the query.
- The table shows as many rows as fit its region and no more; the rest are
  kept but not drawn. The panel is small, usually five to eight rows.
  `LIMIT` the query to what should show.
- Text wider than its cell scrolls, or is cut short where the config says
  `overflow: truncate`. The fonts cover Latin-1; other characters show as
  `?`.
- A table's data keeps its newest 256 rows.

## Answers

| Status | When | Body |
|---|---|---|
| 200 | done | the count, `4 values` or `2 rows`; or what `GET` asked for |
| 400 | the body is not what the path takes | what is wrong, with the row and the column for a table |
| 401 | the token is needed and missing or wrong | `a bearer token is needed` |
| 404 | no chart or table in the config has that name, or another path | which |
| 405 | another method | the ones there are |
| 413 | the body is over 256 KB | |

A refused request changes nothing: the panel keeps what it had. Bodies are
JSON in UTF-8; no `Content-Type` is needed.

## From a database query

A program that runs a query on a timer and pushes the result, with the
standard library only:

```python
import json, os, sqlite3, urllib.request

PANEL = "http://127.0.0.1:4049"

def push(method, path, body):
    request = urllib.request.Request(PANEL + path, data=json.dumps(body).encode(), method=method)
    token = os.environ.get("PANEL_DDP_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request, timeout=5) as response:
        return response.read().decode().strip()

db = sqlite3.connect("metrics.db")
db.row_factory = sqlite3.Row

# A table: a row of the result is a row on the panel.
rows = db.execute(
    "SELECT name, CAST(ROUND(avg_ms) AS INTEGER) AS ms FROM services ORDER BY avg_ms DESC LIMIT 5"
).fetchall()
table = []
for row in rows:
    # A chart in the row: the column is an array of numbers.
    history = db.execute(
        "SELECT ms FROM samples WHERE service = ? ORDER BY at DESC LIMIT 24", (row["name"],)
    ).fetchall()
    table.append({"name": row["name"], "ms": row["ms"], "latency": [h["ms"] for h in reversed(history)]})
print(push("PUT", "/tables/services", table))

# A chart on its own: one column of a query, oldest first.
power = db.execute("SELECT watts FROM power ORDER BY at DESC LIMIT 120").fetchall()
print(push("PUT", "/series/power", [p["watts"] for p in reversed(power)]))
```

`urlopen` raises `urllib.error.HTTPError` on a 4xx; its body says what is
wrong. `tools/push_example.py` is the same with the errors handled.

## Things to know

- **Memory only.** What is pushed is gone when `panel-ddp` restarts. Push
  on a timer, or at least once at the start; a config reload keeps it.
- **Push whole results.** `PUT` is one request that replaces everything,
  so the panel never shows half of an update. Prefer it to a `DELETE` and
  `POST`s.
- **No rate limit, and no need to hurry.** The panel draws ten frames a
  second by default; pushing more often than once a second shows nothing
  more.
- **A page with its own `data` wins.** A page whose config has
  `data.series` or `data.tables` under that name (the demos do) shows that
  instead of what is pushed.
- **Nothing to draw.** A chart without values shows a dim flat line; a
  table without rows shows only its fixed rows.
- **Pushing does not stall the panel.** The endpoint has its own thread
  and answers each request on another.

## Checklist for a program, or an agent, pushing data

1. Find the endpoint: `sources.http.listen` in the panel's config, or ask
   whoever runs it. Get the token if the config has one.
2. `GET /` and read the names and the columns. If the name you need is not
   there, the config needs the chart or table first: see
   [charts.md](charts.md), and run `panel-ddp check` on the config after
   editing it.
3. Shape the data: an array of numbers for a series; for a table an array
   of objects with exactly the listed columns, text already formatted,
   `numbers` columns as arrays.
4. `PUT` it, and check for `200` and the count you expect.
5. Repeat on a timer.
