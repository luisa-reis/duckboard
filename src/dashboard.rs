//! Draws a page: its layers first to last, each over those before it where
//! it draws, unless an alert takes the panel over.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::Alert;
use crate::model::{Backdrop, Content, Page};
use crate::hub;
use crate::palette::{Rgba, BLACK};
use crate::tiles::{self, centred, Ctx};
use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_4X6, iso_8859_1::FONT_5X8, iso_8859_1::FONT_6X10, MonoTextStyle},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};

/// The first alert whose entity is in its state, if any.
fn active<'a>(alerts: &'a [Alert], ctx: &Ctx) -> Option<&'a Alert> {
    alerts.iter().find(|a| ctx.data.sensors.get(&a.entity).is_some_and(|s| s.state == a.state))
}

/// The whole panel in the alert's colour, pulsing, with the label centred
/// on the alert area (the hub's, from today's config files).
fn alert(a: &Alert, area: Rectangle, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
    let period = (a.pulse_seconds * ctx.fps as f32).max(1.0);
    let phase = (ctx.frame as f32 % period) / period * std::f32::consts::TAU;
    let alpha = 0.35 + 0.35 * (0.5 + 0.5 * phase.cos());
    let _ = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
        .into_styled(PrimitiveStyle::with_fill(a.color.scaled(alpha)))
        .draw(c);
    let cx = area.top_left.x + area.size.width as i32 / 2;
    let cy = area.top_left.y + area.size.height as i32 / 2;
    let white = Rgba::rgb(255, 255, 255);
    match a.label.chars().count() {
        0..=3 => centred(c, &a.label, cx, cy - 5, MonoTextStyle::new(&FONT_6X10, white)),
        4 => centred(c, &a.label, cx, cy - 4, MonoTextStyle::new(&FONT_5X8, white)),
        // Longer labels go on the band along the top of the area.
        _ => centred(c, &a.label, cx, area.top_left.y + 2, MonoTextStyle::new(&FONT_4X6, white)),
    }
}

pub fn draw(page: &Page, alerts: &[Alert], alert_area: Rectangle, c: &mut Canvas, ctx: &Ctx) {
    if let Some(a) = active(alerts, ctx) {
        alert(a, alert_area, c, ctx);
        return;
    }
    c.clear(BLACK).unwrap();
    for layer in &page.layers {
        let palette = ctx.palette.with(&layer.colors);
        match &layer.content {
            Content::Tile(spec) => tiles::draw(spec, c, layer.area, ctx, &palette),
            Content::Hub(spec) => hub::draw(spec, c, layer.area, ctx.data, ctx.frame, &palette),
            Content::Backdrop { source, alpha } => backdrop(*source, *alpha, layer.area, c, ctx),
        }
    }
}

/// The panel-sized picture over the area, blended over black at `alpha`
/// and written in place of what was there, nothing else done to it.
fn backdrop(source: Backdrop, alpha: f32, area: Rectangle, c: &mut Canvas, ctx: &Ctx) {
    let full: Option<&[u8]> = match source {
        Backdrop::Media => ctx.data.media.as_ref().and_then(|m| m.art.as_ref()).map(|a| a.full.as_slice()),
        Backdrop::Frame => ctx.picture,
    };
    let Some(full) = full else { return };
    let area = area.intersection(&Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT)));
    let Some(bottom_right) = area.bottom_right() else { return };
    for y in area.top_left.y..=bottom_right.y {
        for x in area.top_left.x..=bottom_right.x {
            let i = (y as usize * WIDTH as usize + x as usize) * 3;
            for (out, &p) in c.px[i..i + 3].iter_mut().zip(&full[i..i + 3]) {
                *out = (p as f32 * alpha) as u8;
            }
        }
    }
}
