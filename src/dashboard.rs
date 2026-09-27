//! Draws the background, then the tiles and the hub in their regions, in
//! ascending `z` so a higher region covers a lower one where they overlap.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::{Alert, Background, Region, Regions, Slot, Tiles};
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
/// on the hub.
fn alert(a: &Alert, hub: Region, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
    let period = (a.pulse_seconds * ctx.fps as f32).max(1.0);
    let phase = (ctx.frame as f32 % period) / period * std::f32::consts::TAU;
    let alpha = 0.35 + 0.35 * (0.5 + 0.5 * phase.cos());
    let _ = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
        .into_styled(PrimitiveStyle::with_fill(a.color.scaled(alpha)))
        .draw(c);
    let area = hub.rect();
    let cx = area.top_left.x + area.size.width as i32 / 2;
    let cy = area.top_left.y + area.size.height as i32 / 2;
    let white = Rgba::rgb(255, 255, 255);
    match a.label.chars().count() {
        0..=3 => centred(c, &a.label, cx, cy - 5, MonoTextStyle::new(&FONT_6X10, white)),
        4 => centred(c, &a.label, cx, cy - 4, MonoTextStyle::new(&FONT_5X8, white)),
        // Longer labels go on the band along the top of the hub.
        _ => centred(c, &a.label, cx, area.top_left.y + 2, MonoTextStyle::new(&FONT_4X6, white)),
    }
}

pub fn draw(tiles: &Tiles, regions: &Regions, alerts: &[Alert], c: &mut Canvas, ctx: &Ctx) {
    if let Some(a) = active(alerts, ctx) {
        alert(a, regions.hub, c, ctx);
        return;
    }
    c.clear(BLACK).unwrap();
    // The background picture over the black panel at its alpha, and
    // nothing else done to it.
    let backdrop: Option<(&[u8], f32)> = match tiles.background {
        Some(Background::Media { alpha }) => {
            ctx.data.media.as_ref().and_then(|m| m.art.as_ref()).map(|a| (a.full.as_slice(), alpha))
        }
        Some(Background::Frame { alpha }) => ctx.picture.map(|p| (p, alpha)),
        _ => None,
    };
    if let Some((full, alpha)) = backdrop {
        for i in 0..(WIDTH * HEIGHT) as usize {
            let p = &full[i * 3..i * 3 + 3];
            c.px[i * 3] = (p[0] as f32 * alpha) as u8;
            c.px[i * 3 + 1] = (p[1] as f32 * alpha) as u8;
            c.px[i * 3 + 2] = (p[2] as f32 * alpha) as u8;
        }
    }
    for (slot, region) in regions.in_order() {
        match tiles.tile(slot) {
            Some(entry) => {
                let palette = ctx.palette.with(&entry.colors);
                tiles::draw(&entry.spec, c, region.rect(), ctx, &palette);
            }
            None => {
                debug_assert_eq!(slot, Slot::Hub);
                let palette = ctx.palette.with(&tiles.hub.colors);
                hub::draw(&tiles.hub.spec, c, region.rect(), ctx.data, ctx.frame, &palette);
            }
        }
    }
}
