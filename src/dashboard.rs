//! Draws a page: its layers first to last, each over those before it where
//! it draws, unless an alert takes the panel over.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::Alert;
use crate::model::Page;
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
        tiles::draw(&layer.tile, c, layer.area, ctx, &layer.palette);
    }
}
