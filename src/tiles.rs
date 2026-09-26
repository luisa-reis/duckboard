//! The tiles in the corners of the panel. Each draws into a 24x24 area;
//! anything that would spill over is clipped to it.

use crate::canvas::Canvas;
use crate::config::TileSpec;
use crate::data::Snapshot;
use crate::icons;
use crate::palette::*;
use crate::weather::Sky;
use chrono::{DateTime, Datelike, Local, Timelike};
use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_4X6, iso_8859_1::FONT_5X8, iso_8859_1::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

/// Everything a tile may draw from.
pub struct Ctx<'a> {
    pub now: DateTime<Local>,
    pub frame: u32,
    pub data: &'a Snapshot,
}

pub fn draw(spec: &TileSpec, c: &mut Canvas, area: Rectangle, ctx: &Ctx) {
    let mut clipped = c.clipped(&area);
    match spec {
        TileSpec::Clock => clock(&mut clipped, area, ctx),
        TileSpec::Date => date(&mut clipped, area, ctx),
        TileSpec::Weather => weather(&mut clipped, area, ctx),
        TileSpec::Sensor { entity, label, unit, decimals } => {
            sensor(&mut clipped, area, ctx, entity, label, unit.as_deref(), *decimals)
        }
        TileSpec::Progress { entity, label, max, decimals } => {
            progress(&mut clipped, area, ctx, entity, label, *max, *decimals)
        }
        TileSpec::NowPlaying => now_playing(&mut clipped, area, ctx),
        TileSpec::Blank => {}
    }
}

/// Text centred on `cx`, its top edge at `top`.
pub fn centred<D: DrawTarget<Color = Rgb888>>(
    t: &mut D,
    text: &str,
    cx: i32,
    top: i32,
    style: MonoTextStyle<Rgb888>,
) {
    let ts = TextStyleBuilder::new().alignment(Alignment::Center).baseline(Baseline::Top).build();
    let _ = Text::with_text_style(text, Point::new(cx, top), style, ts).draw(t);
}

fn fill<D: DrawTarget<Color = Rgb888>>(t: &mut D, r: Rectangle, colour: Rgb888) {
    let _ = r.into_styled(PrimitiveStyle::with_fill(colour)).draw(t);
}

fn clock<D: DrawTarget<Color = Rgb888>>(t: &mut D, area: Rectangle, ctx: &Ctx) {
    let o = area.top_left;
    let cx = o.x + 12;
    let big = MonoTextStyle::new(&FONT_6X10, WHITE);
    centred(t, &format!("{:02}", ctx.now.hour()), cx, o.y + 1, big);
    let big = MonoTextStyle::new(&FONT_6X10, SKY);
    centred(t, &format!("{:02}", ctx.now.minute()), cx, o.y + 12, big);
    // Seconds as a bar along the bottom edge, growing left to right.
    let w = (ctx.now.second() * 24 / 59).max(1);
    fill(t, Rectangle::new(Point::new(o.x, o.y + 23), Size::new(24, 1)), DIM);
    fill(t, Rectangle::new(Point::new(o.x, o.y + 23), Size::new(w, 1)), AMBER);
}

const WEEKDAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
const MONTHS: [&str; 12] =
    ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

fn date<D: DrawTarget<Color = Rgb888>>(t: &mut D, area: Rectangle, ctx: &Ctx) {
    let o = area.top_left;
    let cx = o.x + 12;
    let weekday = WEEKDAYS[ctx.now.weekday().num_days_from_monday() as usize];
    let month = MONTHS[ctx.now.month0() as usize];
    centred(t, weekday, cx, o.y, MonoTextStyle::new(&FONT_5X8, AMBER));
    centred(t, &ctx.now.day().to_string(), cx, o.y + 8, MonoTextStyle::new(&FONT_6X10, WHITE));
    centred(t, month, cx, o.y + 18, MonoTextStyle::new(&FONT_4X6, GREY));
}

fn weather<D: DrawTarget<Color = Rgb888>>(t: &mut D, area: Rectangle, ctx: &Ctx) {
    let o = area.top_left;
    let cx = o.x + 12;
    let Some(w) = &ctx.data.weather else {
        centred(t, "--", cx, o.y + 13, MonoTextStyle::new(&FONT_6X10, DIM));
        return;
    };
    icons::draw(t, Sky::from_code(w.code), w.is_day, o + Point::new((24 - icons::SIZE as i32) / 2, 0));
    let temp = format!("{}°", w.temperature.round() as i32);
    centred(t, &temp, cx, o.y + 13, MonoTextStyle::new(&FONT_6X10, WHITE));
}

/// Pixel width of `text` in a font whose characters are `cw` wide.
fn text_width(text: &str, cw: i32) -> i32 {
    text.chars().count() as i32 * cw - 1
}

fn sensor<D: DrawTarget<Color = Rgb888>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    entity: &str,
    label: &str,
    unit: Option<&str>,
    decimals: Option<u8>,
) {
    let o = area.top_left;
    let cx = o.x + 12;
    centred(t, label, cx, o.y, MonoTextStyle::new(&FONT_4X6, GREY));
    let Some(s) = ctx.data.sensors.get(entity) else {
        centred(t, "--", cx, o.y + 8, MonoTextStyle::new(&FONT_6X10, DIM));
        return;
    };
    let value = match s.state.parse::<f64>() {
        Ok(v) => {
            // Fit 24 pixels: four characters of the big font. Decimals go
            // first, then the font.
            let want = decimals.map(usize::from).unwrap_or(if s.state.contains('.') { 1 } else { 0 });
            let mut text = format!("{v:.want$}");
            if text_width(&text, 6) > 24 {
                text = format!("{v:.0}");
            }
            text
        }
        Err(_) => s.state.to_uppercase(),
    };
    if text_width(&value, 6) <= 24 {
        centred(t, &value, cx, o.y + 8, MonoTextStyle::new(&FONT_6X10, WHITE));
    } else {
        centred(t, &value, cx, o.y + 10, MonoTextStyle::new(&FONT_4X6, WHITE));
    }
    let unit = unit.map(str::to_string).or_else(|| s.unit.clone()).unwrap_or_default();
    centred(t, &unit, cx, o.y + 18, MonoTextStyle::new(&FONT_4X6, GREY));
}

const FULL: Rgb888 = Rgb888::new(60, 220, 90);

/// Label, the value, and a bar along the bottom edge that fills left to
/// right, amber on the way and green once it reaches `max`.
fn progress<D: DrawTarget<Color = Rgb888>>(
    t: &mut D,
    area: Rectangle,
    ctx: &Ctx,
    entity: &str,
    label: &str,
    max: f64,
    decimals: u8,
) {
    let o = area.top_left;
    let cx = o.x + 12;
    centred(t, label, cx, o.y, MonoTextStyle::new(&FONT_4X6, GREY));
    let sensor = ctx.data.sensors.get(entity);
    let value = sensor.and_then(|s| s.state.parse::<f64>().ok());
    let Some(v) = value else {
        centred(t, "--", cx, o.y + 7, MonoTextStyle::new(&FONT_6X10, DIM));
        fill(t, Rectangle::new(Point::new(o.x, o.y + 19), Size::new(24, 4)), DIM);
        return;
    };
    // The value, with a short unit such as "%" in the small font on its
    // baseline; the pair is centred as one. Decimals go if it does not fit.
    let unit = sensor.and_then(|s| s.unit.as_deref()).filter(|u| u.chars().count() <= 2).unwrap_or("");
    let unit_w = if unit.is_empty() { 0 } else { text_width(unit, 4) + 1 };
    let want = usize::from(decimals);
    let mut text = format!("{v:.want$}");
    if text_width(&text, 6) + unit_w > 24 {
        text = format!("{v:.0}");
    }
    let left = cx - (text_width(&text, 6) + unit_w) / 2;
    let ts = TextStyleBuilder::new().alignment(Alignment::Left).baseline(Baseline::Top).build();
    let _ = Text::with_text_style(&text, Point::new(left, o.y + 7), MonoTextStyle::new(&FONT_6X10, WHITE), ts).draw(t);
    if !unit.is_empty() {
        let x = left + text_width(&text, 6) + 1;
        let _ = Text::with_text_style(unit, Point::new(x, o.y + 11), MonoTextStyle::new(&FONT_4X6, GREY), ts).draw(t);
    }
    // The bar: a one-pixel frame around a 22x2 fill.
    let frac = (v / max).clamp(0.0, 1.0);
    let colour = if frac >= 1.0 { FULL } else { AMBER };
    let _ = Rectangle::new(Point::new(o.x, o.y + 19), Size::new(24, 4))
        .into_styled(PrimitiveStyle::with_stroke(GREY, 1))
        .draw(t);
    let w = (frac * 22.0).round() as u32;
    if w > 0 {
        fill(t, Rectangle::new(Point::new(o.x + 1, o.y + 20), Size::new(w, 2)), colour);
    }
}

/// Text on one line, scrolling left when wider than the tile.
fn marquee<D: DrawTarget<Color = Rgb888>>(t: &mut D, text: &str, area: Rectangle, top: i32, frame: u32, colour: Rgb888) {
    let style = MonoTextStyle::new(&FONT_4X6, colour);
    let o = area.top_left;
    let w = text_width(text, 4);
    let width = area.size.width as i32;
    if w <= width {
        centred(t, text, o.x + width / 2, top, style);
        return;
    }
    let gap = 12;
    let offset = (frame / 2) as i32 % (w + gap);
    let ts = TextStyleBuilder::new().alignment(Alignment::Left).baseline(Baseline::Top).build();
    for start in [o.x - offset, o.x - offset + w + gap] {
        let _ = Text::with_text_style(text, Point::new(start, top), style, ts).draw(t);
    }
}

fn now_playing<D: DrawTarget<Color = Rgb888>>(t: &mut D, area: Rectangle, ctx: &Ctx) {
    let o = area.top_left;
    let Some(m) = ctx.data.media.as_ref().filter(|m| m.playing) else {
        centred(t, "--", o.x + 12, o.y + 8, MonoTextStyle::new(&FONT_6X10, DIM));
        return;
    };
    marquee(t, &m.artist, area, o.y + 4, ctx.frame, GREY);
    marquee(t, &m.title, area, o.y + 13, ctx.frame, WHITE);
}
