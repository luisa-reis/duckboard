//! The tiles in the corners of the panel. Each draws into a 24x24 area;
//! anything that would spill over is clipped to it.

use crate::canvas::Canvas;
use crate::config::TileSpec;
use crate::data::Snapshot;
use crate::palette::*;
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
    let w = (ctx.now.second() as u32 * 24 / 59).max(1);
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
