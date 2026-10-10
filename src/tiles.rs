//! The tiles, each drawn into its region (24x24 by default); anything that
//! would spill over is clipped to it. A tile's content is laid out for 24
//! pixels of height and centred in taller or shorter regions (a text tile's
//! line by its font's height); widths follow the region. Colours come from the layer's palette, by role.

use crate::art;
use crate::canvas::Canvas;
use crate::config::{Align, Overflow, Seconds, TextSize, TileSpec, Units};
use crate::data::{Record, Snapshot, Value};
use crate::icons;
use crate::palette::{Palette, Rgba};
use crate::picture::Scaled;
use crate::weather::Sky;
use chrono::{DateTime, Datelike, Local, Timelike};
use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_4X6, iso_8859_1::FONT_5X8, iso_8859_1::FONT_6X10, iso_8859_1 as fonts, MonoFont, MonoTextStyle},
    prelude::*,
    primitives::{Circle, Polyline, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

/// Everything a tile may draw from.
pub struct Ctx<'a> {
    pub now: DateTime<Local>,
    pub frame: u32,
    pub data: &'a Snapshot,
    pub temperature: Units,
    /// Frames per second, for anything timed in seconds.
    pub fps: u32,
    /// The `[frame]` picture due now, for a picture tile.
    pub picture: Option<&'a Scaled>,
}

impl Ctx<'_> {
    /// Time since the run started, in milliseconds, from the frame and the
    /// frame rate: what animations go by, so they keep their speed at any
    /// rate.
    pub fn elapsed_ms(&self) -> u64 {
        self.frame as u64 * 1000 / self.fps.max(1) as u64
    }
}

/// How fast text too wide for its tile scrolls, in pixels a second.
const SCROLL_PX_PER_SECOND: u64 = 5;

/// Draws a tile into its area. `record` is the pushed row a tile on a
/// table's repeat row takes its `column` from.
pub fn draw(spec: &TileSpec, c: &mut Canvas, area: Rectangle, ctx: &Ctx, p: &Palette, record: Option<&Record>) {
    match *spec {
        TileSpec::Art { shape, spin, paused_alpha, corner_alpha, alpha, idle } => {
            let style = art::Style { shape, spin, paused_alpha, corner_alpha, alpha, idle };
            return art::draw(&style, c, area, ctx, p);
        }
        TileSpec::Picture { alpha } => return art::picture(c, area, ctx.picture, alpha),
        _ => {}
    }
    let mut clipped = c.clipped(&area);
    match spec {
        TileSpec::Clock { seconds, dot_size } => clock(&mut clipped, area, ctx, p, *seconds, *dot_size),
        TileSpec::Date => date(&mut clipped, area, ctx, p),
        TileSpec::Text { text, column, size, align, overflow } => {
            let text = match (text, column.as_ref().and_then(|c| record?.get(c))) {
                (Some(text), _) => text.as_str(),
                (None, Some(Value::Text(text))) => text.as_str(),
                _ => "",
            };
            line(&mut clipped, area, ctx, p, text, *size, *align, *overflow)
        }
        TileSpec::Weather => weather(&mut clipped, area, ctx, p),
        TileSpec::Sensor { entity, label, unit, decimals } => {
            sensor(&mut clipped, area, ctx, p, entity, label, unit.as_deref(), *decimals)
        }
        TileSpec::Progress { entity, label, max, decimals } => {
            progress(&mut clipped, area, ctx, p, entity, label, *max, *decimals)
        }
        TileSpec::LineChart { line, dot, .. } => {
            chart(&mut clipped, area, p, chart_values(ctx, spec, record), *line, None, *dot)
        }
        TileSpec::AreaChart { line, area: top, area_bottom, dot, .. } => {
            let top = top.unwrap_or(line.unwrap_or(p.accent).scaled(AREA_ALPHA));
            let fill_with = Some((top, area_bottom.unwrap_or(top)));
            chart(&mut clipped, area, p, chart_values(ctx, spec, record), *line, fill_with, *dot)
        }
        TileSpec::BarChart { bar, last, width, gap, .. } => {
            let bar = bar.unwrap_or(p.accent);
            bars(&mut clipped, area, p, chart_values(ctx, spec, record), bar, last.unwrap_or(bar), *width, *gap)
        }
        TileSpec::BulletChart { min, max, ranges, bar, marker, band, .. } => {
            let (value, target) = bullet_values(ctx, spec, record);
            let colours = (bar.unwrap_or(p.accent), marker.unwrap_or(p.text), band.unwrap_or(p.label));
            bullet(&mut clipped, area, value, target, (*min, *max), ranges, colours)
        }
        TileSpec::NowPlaying => now_playing(&mut clipped, area, ctx, p),
        TileSpec::Art { .. } | TileSpec::Picture { .. } | TileSpec::Blank => {}
    }
}

/// The column a tile centres its content on, and the top of its content,
/// which is laid out for 24 pixels of height and centred vertically.
fn anchor(area: Rectangle) -> (i32, i32) {
    let o = area.top_left;
    (o.x + area.size.width as i32 / 2, o.y + (area.size.height as i32 - 24) / 2)
}

/// Text centred on `cx`, its top edge at `top`.
pub fn centred<D: DrawTarget<Color = Rgba>>(t: &mut D, text: &str, cx: i32, top: i32, style: MonoTextStyle<Rgba>) {
    let ts = TextStyleBuilder::new().alignment(Alignment::Center).baseline(Baseline::Top).build();
    let _ = Text::with_text_style(text, Point::new(cx, top), style, ts).draw(t);
}

fn fill<D: DrawTarget<Color = Rgba>>(t: &mut D, r: Rectangle, colour: Rgba) {
    let _ = r.into_styled(PrimitiveStyle::with_fill(colour)).draw(t);
}

fn clock<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    p: &Palette,
    seconds: Seconds,
    dot_size: u32,
) {
    let (cx, top) = anchor(area);
    centred(t, &format!("{:02}", ctx.now.hour()), cx, top + 2, MonoTextStyle::new(&FONT_6X10, p.text));
    centred(t, &format!("{:02}", ctx.now.minute()), cx, top + 12, MonoTextStyle::new(&FONT_6X10, p.secondary));
    // Seconds on a ring around the tile, the largest circle that fits,
    // clockwise from twelve o'clock, one step per second: either the ring
    // fills up to the second, or a single dot sits at it. The ring's pixels
    // come from the circle itself, so the track and the accent agree.
    let d = area.size.width.min(area.size.height);
    let o = area.top_left + Point::new((area.size.width - d) as i32 / 2, (area.size.height - d) as i32 / 2);
    let ring = Circle::new(o, d).into_styled(PrimitiveStyle::with_stroke(p.track, 1));
    let target = ctx.now.second() as f32 / 60.0 * std::f32::consts::TAU;
    let centre = (d as f32 - 1.0) / 2.0;
    let angle_of = |q: Point| {
        let dx = (q.x - o.x) as f32 - centre;
        let dy = (q.y - o.y) as f32 - centre;
        let a = dx.atan2(-dy); // 0 at twelve, clockwise
        if a < 0.0 { a + std::f32::consts::TAU } else { a }
    };
    match seconds {
        Seconds::Ring => {
            let _ = t.draw_iter(ring.pixels().map(|Pixel(q, _)| {
                Pixel(q, if angle_of(q) < target { p.accent } else { p.track })
            }));
        }
        Seconds::Dot => {
            let _ = ring.draw(t);
            // The `dot_size` ring pixels nearest the second, going the short
            // way round.
            let distance = |q: Point| {
                let d = (angle_of(q) - target).abs();
                d.min(std::f32::consts::TAU - d)
            };
            let mut px: Vec<Point> = ring.pixels().map(|Pixel(q, _)| q).collect();
            px.sort_by(|a, b| distance(*a).total_cmp(&distance(*b)));
            let _ = t.draw_iter(px.into_iter().take(dot_size as usize).map(|q| Pixel(q, p.accent)));
        }
    }
}

const WEEKDAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

fn date<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, ctx: &Ctx, p: &Palette) {
    let (cx, top) = anchor(area);
    let weekday = WEEKDAYS[ctx.now.weekday().num_days_from_monday() as usize];
    let month = MONTHS[ctx.now.month0() as usize];
    centred(t, weekday, cx, top, MonoTextStyle::new(&FONT_5X8, p.accent));
    centred(t, &ctx.now.day().to_string(), cx, top + 8, MonoTextStyle::new(&FONT_6X10, p.text));
    centred(t, month, cx, top + 18, MonoTextStyle::new(&FONT_4X6, p.label));
}

fn weather<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, ctx: &Ctx, p: &Palette) {
    let (cx, top) = anchor(area);
    let Some(w) = &ctx.data.weather else {
        centred(t, "--", cx, top + 13, MonoTextStyle::new(&FONT_6X10, p.track));
        return;
    };
    let icon_x = area.top_left.x + (area.size.width as i32 - icons::SIZE as i32) / 2;
    icons::draw(t, Sky::from_code(w.code), w.is_day, Point::new(icon_x, top), p);
    let temp = format!("{}°", w.temperature.round() as i32);
    centred(t, &temp, cx, top + 13, MonoTextStyle::new(&FONT_6X10, p.text));
}

/// Pixel width of `text` in a font whose characters are `cw` wide.
fn text_width(text: &str, cw: i32) -> i32 {
    text.chars().count() as i32 * cw - 1
}

#[allow(clippy::too_many_arguments)]
fn sensor<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    p: &Palette,
    entity: &str,
    label: &str,
    unit: Option<&str>,
    decimals: Option<u8>,
) {
    let (cx, top) = anchor(area);
    let width = area.size.width as i32;
    centred(t, label, cx, top, MonoTextStyle::new(&FONT_4X6, p.label));
    let Some(s) = ctx.data.sensors.get(entity) else {
        centred(t, "--", cx, top + 8, MonoTextStyle::new(&FONT_6X10, p.track));
        return;
    };
    // A reading in degrees follows the configured temperature unit.
    let mut shown_unit = s.unit.clone();
    let value = match s.state.parse::<f64>() {
        Ok(mut v) => {
            if let Some(u) = &s.unit {
                let (cv, cu) = ctx.temperature.convert(v, u);
                v = cv;
                shown_unit = Some(cu);
            }
            // Fit the tile's width (four characters of the big font in 24
            // pixels). Decimals go first, then the font.
            let want = decimals.map(usize::from).unwrap_or(if s.state.contains('.') { 1 } else { 0 });
            let mut text = format!("{v:.want$}");
            if text_width(&text, 6) > width {
                text = format!("{v:.0}");
            }
            text
        }
        Err(_) => s.state.to_uppercase(),
    };
    if text_width(&value, 6) <= width {
        centred(t, &value, cx, top + 8, MonoTextStyle::new(&FONT_6X10, p.text));
    } else {
        centred(t, &value, cx, top + 10, MonoTextStyle::new(&FONT_4X6, p.text));
    }
    let unit = unit.map(str::to_string).or(shown_unit).unwrap_or_default();
    centred(t, &unit, cx, top + 18, MonoTextStyle::new(&FONT_4X6, p.label));
}

/// Label, the value, and a bar across the tile's width under them that
/// fills left to right, accent on the way and full once it reaches `max`.
#[allow(clippy::too_many_arguments)]
fn progress<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    p: &Palette,
    entity: &str,
    label: &str,
    max: f64,
    decimals: u8,
) {
    let (cx, top) = anchor(area);
    let (left_edge, width) = (area.top_left.x, area.size.width);
    let bar = Rectangle::new(Point::new(left_edge, top + 19), Size::new(width, 4));
    centred(t, label, cx, top, MonoTextStyle::new(&FONT_4X6, p.label));
    let sensor = ctx.data.sensors.get(entity);
    let value = sensor.and_then(|s| s.state.parse::<f64>().ok());
    let Some(v) = value else {
        centred(t, "--", cx, top + 7, MonoTextStyle::new(&FONT_6X10, p.track));
        fill(t, bar, p.track);
        return;
    };
    // The value, with a short unit such as "%" in the small font on its
    // baseline; the pair is centred as one. Decimals go if it does not fit.
    let unit = sensor.and_then(|s| s.unit.as_deref()).filter(|u| u.chars().count() <= 2).unwrap_or("");
    let unit_w = if unit.is_empty() { 0 } else { text_width(unit, 4) + 1 };
    let want = usize::from(decimals);
    let mut text = format!("{v:.want$}");
    if text_width(&text, 6) + unit_w > width as i32 {
        text = format!("{v:.0}");
    }
    let left = cx - (text_width(&text, 6) + unit_w) / 2;
    let ts = TextStyleBuilder::new().alignment(Alignment::Left).baseline(Baseline::Top).build();
    let _ = Text::with_text_style(&text, Point::new(left, top + 7), MonoTextStyle::new(&FONT_6X10, p.text), ts).draw(t);
    if !unit.is_empty() {
        let x = left + text_width(&text, 6) + 1;
        let _ = Text::with_text_style(unit, Point::new(x, top + 11), MonoTextStyle::new(&FONT_4X6, p.label), ts).draw(t);
    }
    // The bar: a one-pixel frame around a two-pixel-high fill.
    let frac = (v / max).clamp(0.0, 1.0);
    let colour = if frac >= 1.0 { p.full } else { p.accent };
    let _ = bar.into_styled(PrimitiveStyle::with_stroke(p.label, 1)).draw(t);
    let w = (frac * width.saturating_sub(2) as f64).round() as u32;
    if w > 0 {
        fill(t, Rectangle::new(Point::new(left_edge + 1, top + 20), Size::new(w, 2)), colour);
    }
}

/// Text on one line, scrolling left when wider than the tile.
fn marquee<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    text: &str,
    area: Rectangle,
    top: i32,
    ctx: &Ctx,
    font: &MonoFont<'static>,
    colour: Rgba,
) {
    let style = MonoTextStyle::new(font, colour);
    let o = area.top_left;
    let w = text_width(text, font.character_size.width as i32);
    let width = area.size.width as i32;
    if w <= width {
        centred(t, text, o.x + width / 2, top, style);
        return;
    }
    let gap = 12;
    let offset = (ctx.elapsed_ms() * SCROLL_PX_PER_SECOND / 1000 % (w + gap) as u64) as i32;
    let ts = TextStyleBuilder::new().alignment(Alignment::Left).baseline(Baseline::Top).build();
    for start in [o.x - offset, o.x - offset + w + gap] {
        let _ = Text::with_text_style(text, Point::new(start, top), style, ts).draw(t);
    }
}

fn font(size: TextSize) -> &'static MonoFont<'static> {
    match size {
        TextSize::S4X6 => &fonts::FONT_4X6,
        TextSize::S5X7 => &fonts::FONT_5X7,
        TextSize::S5X8 => &fonts::FONT_5X8,
        TextSize::S6X9 => &fonts::FONT_6X9,
        TextSize::S6X10 => &fonts::FONT_6X10,
        TextSize::S6X12 => &fonts::FONT_6X12,
        TextSize::S6X13 => &fonts::FONT_6X13,
        TextSize::S7X13 => &fonts::FONT_7X13,
        TextSize::S7X14 => &fonts::FONT_7X14,
        TextSize::S8X13 => &fonts::FONT_8X13,
        TextSize::S9X15 => &fonts::FONT_9X15,
        TextSize::S9X18 => &fonts::FONT_9X18,
        TextSize::S10X20 => &fonts::FONT_10X20,
    }
}

/// As much of `text` as fits `width` pixels in characters `cw` wide: all of
/// it, or its first whole characters.
fn truncated(text: &str, cw: i32, width: i32) -> &str {
    let fit = ((width + 1) / cw).max(0) as usize;
    text.char_indices().nth(fit).map_or(text, |(end, _)| &text[..end])
}

/// A text tile: one line in the font of `size`, centred in the area's
/// height by the font's own and set against the side `align` says. Wider
/// than the area, it scrolls or is cut short, as `overflow` says.
#[allow(clippy::too_many_arguments)]
fn line<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    p: &Palette,
    text: &str,
    size: TextSize,
    align: Align,
    overflow: Overflow,
) {
    let font = font(size);
    let cw = font.character_size.width as i32;
    let (left, width) = (area.top_left.x, area.size.width as i32);
    let top = area.top_left.y + (area.size.height as i32 - font.character_size.height as i32) / 2;
    if overflow == Overflow::Scroll && text_width(text, cw) > width {
        return marquee(t, text, area, top, ctx, font, p.text);
    }
    let text = truncated(text, cw, width);
    let style = MonoTextStyle::new(font, p.text);
    let x = match align {
        Align::Left => left,
        Align::Center => return centred(t, text, left + width / 2, top, style),
        Align::Right => left + width - text_width(text, cw),
    };
    let ts = TextStyleBuilder::new().alignment(Alignment::Left).baseline(Baseline::Top).build();
    let _ = Text::with_text_style(text, Point::new(x, top), style, ts).draw(t);
}

/// `values` as `cols` values: the mean of each stretch when there are
/// more, a straight line between neighbours when there are fewer.
fn columns(values: &[f64], cols: usize) -> Vec<f64> {
    let n = values.len();
    if n >= cols {
        (0..cols)
            .map(|i| {
                let stretch = &values[i * n / cols..((i + 1) * n / cols).max(i * n / cols + 1)];
                stretch.iter().sum::<f64>() / stretch.len() as f64
            })
            .collect()
    } else {
        (0..cols)
            .map(|i| {
                let at = i as f64 * (n - 1) as f64 / (cols - 1) as f64;
                let (k, f) = (at as usize, at.fract());
                values[k] + (values[(k + 1).min(n - 1)] - values[k]) * f
            })
            .collect()
    }
}

/// The alpha of an area chart's area when it has no colour of its own,
/// against the line's.
const AREA_ALPHA: f32 = 0.35;

/// The values a chart draws: an entity's history, a pushed series, or a
/// column of the pushed row it is on.
fn chart_values<'a>(ctx: &Ctx<'a>, spec: &TileSpec, record: Option<&'a Record>) -> Option<&'a [f64]> {
    if let Some(column) = spec.column() {
        return match record?.get(column)? {
            Value::Series(values) => Some(values),
            Value::Text(_) => None,
        };
    }
    let values = match spec.chart()? {
        (Some(entity), _, hours) => ctx.data.series.get(&(entity.clone(), hours)),
        (None, Some(name), _) => ctx.data.pushed.get(name),
        (None, None, _) => None,
    };
    values.map(|v| v.as_slice())
}

/// How tall each of `values` stands in `rows` pixels, counted from zero, or
/// from the lowest value when one is negative: the highest fills the rows,
/// and none is under a pixel, so a bar shows where a value is.
fn bar_heights(values: &[f64], rows: u32) -> Vec<u32> {
    let base = values.iter().copied().fold(0.0, f64::min);
    let high = values.iter().copied().fold(base, f64::max);
    values
        .iter()
        .map(|v| {
            let up = if high > base { (v - base) / (high - base) } else { 0.0 };
            ((up * rows as f64).round() as u32).clamp(1, rows.max(1))
        })
        .collect()
}

/// A series as bars `width` wide and `gap` apart, as many as fit across
/// the area, centred in it, the latest one in `last`; a flat line in the
/// track colour on the bottom row while there is none.
#[allow(clippy::too_many_arguments)]
fn bars<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    p: &Palette,
    values: Option<&[f64]>,
    bar: Rgba,
    last: Rgba,
    width: u32,
    gap: u32,
) {
    let o = area.top_left;
    let bottom = o.y + area.size.height as i32;
    let Some(values) = values.filter(|v| !v.is_empty()) else {
        fill(t, Rectangle::new(Point::new(o.x, bottom - 1), Size::new(area.size.width, 1)), p.track);
        return;
    };
    let width = width.clamp(1, area.size.width.max(1));
    let count = ((area.size.width + gap) / (width + gap)).max(1);
    let left = o.x + (area.size.width as i32 - (count * (width + gap) - gap) as i32) / 2;
    let heights = bar_heights(&columns(values, count as usize), area.size.height);
    for (i, height) in heights.iter().enumerate() {
        let x = left + (i as u32 * (width + gap)) as i32;
        let colour = if i + 1 == heights.len() { last } else { bar };
        fill(t, Rectangle::new(Point::new(x, bottom - *height as i32), Size::new(width, *height)), colour);
    }
}

/// The colour `at` of the way from `from` (0) to `to` (1), alpha included.
fn blend(from: Rgba, to: Rgba, at: f32) -> Rgba {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * at).round() as u8;
    Rgba { r: mix(from.r, to.r), g: mix(from.g, to.g), b: mix(from.b, to.b), a: mix(from.a, to.a) }
}

/// A series as a line across the area, its lowest value on the
/// bottom row and its highest on the top one, with a dot on its end when
/// `dot` is set; a flat line in the track colour while there is none.
/// `fill` colours what is under the line, down to the bottom of the area:
/// its first colour on the area's top row, its second on the bottom one,
/// and a gradient between when they differ.
fn chart<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    p: &Palette,
    values: Option<&[f64]>,
    line: Option<Rgba>,
    fill_with: Option<(Rgba, Rgba)>,
    dot: Option<Rgba>,
) {
    let o = area.top_left;
    let Some(values) = values.filter(|v| !v.is_empty()) else {
        fill(t, Rectangle::new(o + Point::new(0, area.size.height as i32 / 2), Size::new(area.size.width, 1)), p.track);
        return;
    };
    // The dot is three pixels across, so with one the line keeps a pixel
    // clear of every edge.
    let pad = if dot.is_some() { 1 } else { 0 };
    let cols = area.size.width.saturating_sub(2 * pad).max(1);
    let rows = area.size.height.saturating_sub(2 * pad).max(1);
    let values = columns(values, cols as usize);
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let points: Vec<Point> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let up = if high > low { (v - low) / (high - low) } else { 0.5 };
            o + Point::new(pad as i32 + i as i32, pad as i32 + ((1.0 - up) * (rows - 1) as f64).round() as i32)
        })
        .collect();
    if let Some((top, bottom)) = fill_with {
        let height = area.size.height as i32;
        let colour_of = |y: i32| blend(top, bottom, (y - o.y) as f32 / (height - 1).max(1) as f32);
        let under = points.iter().flat_map(|q| (q.y + 1..o.y + height).map(|y| Pixel(Point::new(q.x, y), colour_of(y))));
        let _ = t.draw_iter(under);
    }
    let colour = line.unwrap_or(p.accent);
    if let [only] = points[..] {
        let _ = Pixel(only, colour).draw(t);
    } else {
        let _ = Polyline::new(&points).into_styled(PrimitiveStyle::with_stroke(colour, 1)).draw(t);
    }
    if let (Some(dot), Some(&end)) = (dot, points.last()) {
        fill(t, Rectangle::with_center(end, Size::new(3, 3)), dot);
    }
}

/// A number in a pushed row's column: one written as text, or the latest
/// of a chart's values.
fn number(record: Option<&Record>, column: &str) -> Option<f64> {
    match record?.get(column)? {
        Value::Text(text) => text.trim().parse().ok(),
        Value::Series(values) => values.last().copied(),
    }
    .filter(|v: &f64| v.is_finite())
}

/// A bullet chart's value and its target, from wherever each comes.
fn bullet_values(ctx: &Ctx, spec: &TileSpec, record: Option<&Record>) -> (Option<f64>, Option<f64>) {
    let TileSpec::BulletChart { entity, series, column, target, target_column, .. } = spec else { return (None, None) };
    let value = match (entity, series, column) {
        (Some(entity), _, _) => ctx.data.sensors.get(entity).and_then(|s| s.state.trim().parse().ok()),
        (_, Some(name), _) => ctx.data.pushed.get(name).and_then(|v| v.last().copied()),
        (_, _, Some(column)) => number(record, column),
        _ => None,
    };
    let target = target.or_else(|| number(record, target_column.as_ref()?));
    (value.filter(|v: &f64| v.is_finite()), target)
}

/// The alpha of a bullet chart's bands against their colour: the first
/// the strongest, as the darkest is in print.
const BAND_ALPHAS: [f32; 5] = [0.55, 0.38, 0.26, 0.18, 0.12];

/// How tall the bar and the marker of a bullet chart are in an area
/// `height` high: the bar about a third of it and the marker about two
/// thirds, each leaving the same above as below.
fn bullet_heights(height: u32) -> (u32, u32) {
    if height < 3 {
        return (height, height);
    }
    let third = height.div_ceil(3);
    let bar = if (height - third) % 2 == 1 { third + 1 } else { third };
    let marker = if height < 5 { height } else { bar + 2 * ((height - bar) / 2).div_ceil(2) };
    (bar, marker.min(height))
}

/// A bullet graph across the area: the bands over its whole height, each
/// to where the next range begins; the bar from the left to `value`; and a
/// marker at `target`. Without a value there is no bar.
fn bullet<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    value: Option<f64>,
    target: Option<f64>,
    (min, max): (f64, f64),
    ranges: &[f64],
    (bar, marker, band): (Rgba, Rgba, Rgba),
) {
    let (o, width, height) = (area.top_left, area.size.width, area.size.height);
    // How far along the area a value is, in pixels from its left.
    let along = |v: f64| (((v - min) / (max - min)).clamp(0.0, 1.0) * width as f64).round() as u32;
    let mut from = 0;
    for (i, end) in ranges.iter().copied().chain([max]).enumerate() {
        let to = along(end);
        let colour = band.scaled(BAND_ALPHAS[i.min(BAND_ALPHAS.len() - 1)]);
        fill(t, Rectangle::new(o + Point::new(from as i32, 0), Size::new(to.saturating_sub(from), height)), colour);
        from = to;
    }
    let (bar_height, marker_height) = bullet_heights(height);
    if let Some(value) = value {
        let top = o.y + ((height - bar_height) / 2) as i32;
        fill(t, Rectangle::new(Point::new(o.x, top), Size::new(along(value), bar_height)), bar);
    }
    if let Some(target) = target {
        // Two pixels wide where there is room, and never off the end.
        let thick = if width >= 40 { 2 } else { 1 };
        let x = along(target).saturating_sub(thick / 2).min(width.saturating_sub(thick));
        let top = o.y + ((height - marker_height) / 2) as i32;
        fill(t, Rectangle::new(Point::new(o.x + x as i32, top), Size::new(thick, marker_height)), marker);
    }
}

fn now_playing<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, ctx: &Ctx, p: &Palette) {
    let (cx, top) = anchor(area);
    let Some(m) = ctx.data.media.as_ref().filter(|m| m.playing) else {
        centred(t, "--", cx, top + 8, MonoTextStyle::new(&FONT_6X10, p.track));
        return;
    };
    marquee(t, &m.artist, area, top + 4, ctx, &FONT_4X6, p.label);
    marquee(t, &m.title, area, top + 13, ctx, &FONT_4X6, p.text);
}

#[cfg(test)]
mod tests {
    use super::{bar_heights, blend, bullet_heights, columns, truncated};
    use crate::palette::Rgba;

    #[test]
    fn truncates_to_whole_characters() {
        assert_eq!(truncated("KITCHEN", 4, 27), "KITCHEN", "seven of four, less the last gap");
        assert_eq!(truncated("KITCHEN", 4, 26), "KITCHE");
        assert_eq!(truncated("°C today", 6, 12), "°C", "by character, not byte");
        assert_eq!(truncated("abc", 6, 3), "");
    }

    #[test]
    fn a_bullets_bar_and_marker_sit_in_the_middle() {
        for height in 1..=24u32 {
            let (bar, marker) = bullet_heights(height);
            assert!(bar >= 1 && bar <= marker && marker <= height, "{height}: {bar} {marker}");
            assert!((height - bar) % 2 == 0 && (height - marker) % 2 == 0, "{height}: as much above as below");
        }
        assert_eq!(bullet_heights(7), (3, 5));
        assert_eq!(bullet_heights(10), (4, 8));
        assert_eq!(bullet_heights(2), (2, 2));
    }

    #[test]
    fn bars_stand_on_zero() {
        assert_eq!(bar_heights(&[0.0, 5.0, 10.0], 20), [1, 10, 20], "from zero, and never under a pixel");
        assert_eq!(bar_heights(&[90.0, 100.0], 10), [9, 10], "not from the lowest");
        assert_eq!(bar_heights(&[-10.0, 0.0, 10.0], 10), [1, 5, 10], "from the lowest when one is negative");
        assert_eq!(bar_heights(&[0.0, 0.0], 10), [1, 1]);
    }

    #[test]
    fn blends_every_channel() {
        let (from, to) = (Rgba { r: 0, g: 100, b: 255, a: 255 }, Rgba { r: 200, g: 100, b: 55, a: 0 });
        assert_eq!(blend(from, to, 0.0), from);
        assert_eq!(blend(from, to, 1.0), to);
        assert_eq!(blend(from, to, 0.5), Rgba { r: 100, g: 100, b: 155, a: 128 });
    }

    #[test]
    fn columns_average_or_stretch() {
        assert_eq!(columns(&[1.0, 3.0, 5.0, 7.0], 2), [2.0, 6.0], "the mean of each stretch");
        assert_eq!(columns(&[1.0, 3.0], 5), [1.0, 1.5, 2.0, 2.5, 3.0], "a straight line between");
        assert_eq!(columns(&[4.0], 3), [4.0, 4.0, 4.0]);
        assert_eq!(columns(&[1.0, 2.0, 3.0], 3), [1.0, 2.0, 3.0]);
    }
}
