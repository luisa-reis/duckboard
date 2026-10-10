# Designing a dashboard, with an agent or by hand

How to get from "I want to see X on the panel" to a page that looks right,
quickly. It is written so that an AI agent can be pointed at it and do the
work: everything is one YAML file, a checker that names mistakes, and a
command that draws any page to a PNG the agent can look at.

The reference for each tile is in [charts.md](charts.md) (text, charts,
tables) and [dashboard.md](dashboard.md) (everything else);
[pushing-data.md](pushing-data.md) covers data sent by another program.

## Asking an agent

Point it here and say what you want to see. For example:

> Read `docs/designing-dashboards.md` in the panel-ddp repository and
> follow its loop. In `dashboard.yaml`, add a page called `energy` showing
> today's solar production as a large number with an area chart under it,
> and house consumption and battery level as two smaller rows. The values
> come from Home Assistant: `sensor.solar_power`, `sensor.house_power`,
> `sensor.battery_level`. Show me the preview when it passes `check`.

The more of these you give, the fewer rounds it takes:

- what to show, and which is the most important thing on the page;
- where each value comes from: a Home Assistant entity id, a pushed series
  or table (and who pushes it), or fixed text;
- which file to edit and what to call the page;
- colours, if you care.

## The loop

```sh
target/release/panel-ddp check dashboard.yaml                                # 1. is it right
target/release/panel-ddp preview --config dashboard.yaml --page energy --out preview.png   # 2. draw it
# 3. look at preview.png, change the YAML, go to 1
```

1. **Edit** the config: a layout, tiles, a page.
2. **`check`** it. A mistake is named with its page, region, row and tile:
   `page energy: region chart: row 2, tile 1: must fit the row, 62 wide and
   10 high`. Fix and repeat until it says `ok`.
3. **`preview --page NAME`** draws that page to a PNG at four times its
   size (256×256 for a 64×64 panel), on made-up data. Open the PNG and
   look at it; an agent should read the image, not assume.
4. **Judge it** against the list under "What to look for", change the
   YAML, and go round again.

Other ways to look:

```sh
target/release/panel-ddp preview --config dashboard.yaml --all --out pages.png     # every page: pages-NAME.png
target/release/panel-ddp preview --config dashboard.yaml --page energy --out p.gif  # animated, for scrolling text
target/release/panel-ddp preview --config dashboard.yaml --page energy --alert      # with the alert up
```

If the build is missing, `cargo build --release` makes
`target/release/panel-ddp`. A running panel picks a saved config up within
a second, so the last step can also be to look at the panel.

### What the preview draws with

- A page's own `data` (see below) when it has some.
- Otherwise sample values: every chart the same waves, every sensor a
  number sweeping to 100, every pushed table column `--`, a gradient for
  album art.
- The time is now. Greyed pixels, if any, are ones the panel's diffuser
  hides (from `2d-gaps.json`); keep content off them.

So a preview shows layout and colour truthfully, and real values only when
the page supplies them.

### Designing with real-looking values

Give the page a `data` block while designing, with values like the real
ones, their widest case especially:

```yaml
pages:
  energy:
    layout: reading
    tiles: {name: solar-name, value: solar-value, chart: solar-chart}
    data:
      series: {sensor.solar_power: [0, 0.4, 1.2, 2.6, 3.4, 3.1, 2.2, 0.9, 0.1]}
      sensors: {sensor.battery_level: {state: "100", unit: "%"}}
      tables: {rooms: [{room: CONSERVATORY, temp: "-12.5"}]}
```

`series` is by entity id or series name, `sensors` by entity id, `tables`
by a table's `data` name. **Take the `data` block out when the design is
done**: a page's own data wins over the live and the pushed values, so a
page left with it shows the made-up numbers for ever.

## The canvas

- The panel is 64×64 pixels unless the config's `width` and `height` say
  otherwise. `x` runs right and `y` down from the top left corner, both
  from 0. Every pixel is one LED: there is no anti-aliasing and no
  sub-pixel anything.
- A **layout** is named regions (`x`, `y`, `width`, `height`, and `z` to
  stack). A **page** puts a tile in each region it uses. Regions may
  overlap, and have no background: a tile only covers what it draws on, so
  text can sit over a chart.
- Anything that does not fit its region is clipped to it, silently.
- Leave a pixel of margin at the panel's edges when you can
  (`x: 1, width: 62`), and one or two between things that should read as
  separate.

### How much text fits

A line of *n* characters is *n* × the font's width − 1 pixels wide, and the
font's height tall.

| `size` | Characters in 64 px | in 62 px | in 30 px | Good for |
|---|---|---|---|---|
| `4x6` | 16 | 15 | 7 | labels, table text, footers |
| `5x7`, `5x8` | 13 | 12 | 6 | table values, short labels |
| `6x9`, `6x10`, `6x12`, `6x13` | 10 | 10 | 5 | values |
| `7x13`, `7x14` | 9 | 9 | 4 | headings |
| `8x13` | 8 | 7 | 3 | |
| `9x15`, `9x18` | 7 | 7 | 3 | a big number |
| `10x20` | 6 | 6 | 3 | the one number that matters |

- Count the widest value the tile will ever show: `-12.5°` is six
  characters, `100%` four.
- Give a text region the font's height or a little more; the line is
  centred in it.
- Too wide, a line scrolls. That suits a song title and does not suit a
  number: make the region wider, the font smaller, or the text shorter, or
  use `overflow: truncate` for names in a table.
- Upper case reads best at `4x6`. Only Latin-1 characters draw.

### How much chart fits

- A line or area chart is readable from about 12×8 pixels, as a hint of a
  trend, and comfortable from 30×15. One with a `dot` needs a pixel more
  on every side.
- A bar chart shows (the region's width + `gap`) ÷ (`width` + `gap`) bars,
  rounded down: 21 in 62 pixels at the default `width: 2, gap: 1`, 7 at
  `width: 6, gap: 3`.
- A bullet chart reads from 5 pixels high and well at 7 to 9, and wants
  width: 40 pixels or more, so that a step of the scale is a pixel.
- A table row needs the height of its tallest text plus a pixel or two:
  10-pixel rows with a 1-pixel `gap` put a header and five rows on a 64
  high panel.

## Colour on an LED panel

- Black is off, and most of a good page is black. Bright text and thin
  lines on black read from across the room; large filled areas glare and
  wash out what is next to them.
- One bright thing a page: the value white (`#ffffff`), its label grey
  (`#6e6e6e`, the `label` role), a chart in one colour.
- Use colour to mean something. The palette's own are amber `#ffaa00`
  (the `accent` role, a chart's line unless it says), blue `#50aaff` and
  green `#3cdc5a`; the demos add a red, `#ff4060`, for a dot.
- Colours are `#rrggbb` or `#rrggbbaa`; a low alpha makes a faint
  background (`#3cdc5a70` for a chart behind text) or fades an area
  (`area_bottom: "#ffaa0000"`).
- Set a colour for everything at once with `schemes`, for a table with its
  `colors`, for one tile with the tile's `colors`: see Colours in
  [dashboard.md](dashboard.md).
- The preview is brighter and sharper than the panel. Dim greys under
  about `#2d2d2d` may not light at all at low brightness.

## Layouts to start from

`demo-dashboards.yaml` has each of these as a working page; copy the
layout and the tiles and change the text and the sources. Render them all
with `preview --config demo-dashboards.yaml --all --out demo.png`.

| Layout | Looks like | Pages using it |
|---|---|---|
| `reading` | a name, one large value, a chart under it, a footer | `outside`, `solar`, `rain` |
| `rows` | three readings, each a name over its value with a chart beside it | `energy`, `network`, `usage` |
| `home` | the clock and the date over one reading | `home` |
| `gauge` | one large value over a bullet chart the width of the panel | `budget` |
| `over` | text over a chart the size of the panel | `week` |
| `sheet` | one region for a table | `rooms` (rows from data), `energy-table`, `targets` (bullet charts from data) |

One reading, as a whole config to try:

```yaml
target: "4.3.2.1"

layouts:
  reading:
    name: {x: 0, y: 2, width: 64, height: 6}
    value: {x: 0, y: 10, width: 64, height: 20}
    chart: {x: 2, y: 33, width: 60, height: 21}
    foot: {x: 0, y: 57, width: 64, height: 6}

tiles:
  solar-name: {kind: text, text: "SOLAR TODAY", size: 4x6, colors: {text: "#6e6e6e"}}
  solar-value: {kind: text, text: "3.2kW", size: 10x20}
  solar-chart: {kind: area_chart, entity: sensor.solar_power, area: "#ffaa00a0", area_bottom: "#ffaa0000", dot: "#ffffff"}
  solar-foot: {kind: text, text: "PEAK 3.4 KW", size: 4x6, colors: {text: "#6e6e6e"}}

pages:
  solar:
    layout: reading
    tiles: {name: solar-name, value: solar-value, chart: solar-chart, foot: solar-foot}
    data:
      series: {sensor.solar_power: [0, 0.2, 0.9, 1.8, 2.9, 3.4, 3.2, 2.4, 1.1, 0.2]}
```

## Which tile shows a value that changes

| The value is | Use | Notes |
|---|---|---|
| fixed words | `text` | any size, aligned |
| a Home Assistant entity's state | `sensor` or `progress` | their own 24-pixel layout: label, value, unit |
| a Home Assistant entity's history | `line_chart`, `area_chart`, `bar_chart` with `entity` | fetched once a minute |
| one number against a target, or against good and bad | `bullet_chart` with `entity`, `series` or `column` | in place of a gauge; `target` and `ranges` in the config |
| numbers another program has | a chart with `series` | pushed to `/series/NAME` |
| text another program has, a query's result | a `table` with `data` and a `repeat` row of `text` tiles with `column` | pushed to `/tables/NAME`; one row and one column is fine for a single value |
| the time, the date, the weather, what is playing | `clock`, `date`, `weather`, `now_playing`, `art` | |

A `text` tile's own text never changes while the panel runs. A number that
must be both free-form (any font, anywhere) and live goes through a table
with `data`, even when it is the only thing in it.

## What to look for

Before calling a page done, on the PNG:

- Nothing is cut off at a region's edge, and nothing scrolls that should
  hold still. Check with the widest values.
- Things that belong together are close; things that do not have a pixel
  or two between them. Nothing touches the panel's edge without meaning
  to.
- Text columns line up: names `align: left`, numbers `align: right`.
- There is one thing the eye goes to first, and it is the right one.
- Labels are dimmer than values.
- A chart has room to show a shape; if it is a smear, give it more pixels
  or fewer values.
- No tile sits on greyed (hidden) pixels.

And in the file:

- `panel-ddp check` says `ok`.
- The page's design-time `data` is gone, unless the page is a demo.
- Every tile's source is configured: `sources.home_assistant` for
  `entity`, `sources.http` for `series` and for tables with `data`.
  `check` refuses a tile whose source is missing.

## Rules for an agent working here

- Edit the config the user named. `dashboard.yaml` is theirs and is
  git-ignored: never commit it, and do not rewrite parts of it you were
  not asked about.
- Never read or print `secrets.yaml`. A config refers to a secret by name,
  `token: {secret: name}`, and never holds one.
- Run `check` after every edit, and `preview` before saying a page is
  done; look at the image.
- Take size from the regions: do not assume 64×64 if the config sets
  `width` and `height`.
- New tile names and page names are yours to choose; keep the user's
  existing ones.
- If what is asked for does not fit (a twelve-character value at `10x20`),
  say so and offer the nearest thing that does, rather than letting it
  scroll or clip.
- Changing the program itself is a different job: this guide is about
  configs. The repository's `CLAUDE.md` covers the code.
