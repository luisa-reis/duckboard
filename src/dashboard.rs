//! Puts the tiles in the corners and the hub in the middle, over the
//! background.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::{Alert, Background, Tiles};
use crate::hub;
use crate::mask::{hub as hub_area, Tile};
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

/// The whole panel in the alert's colour, pulsing, with the label in the
/// middle.
fn alert(a: &Alert, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
    let period = (a.pulse_seconds * ctx.fps as f32).max(1.0);
    let phase = (ctx.frame as f32 % period) / period * std::f32::consts::TAU;
    let alpha = 0.35 + 0.35 * (0.5 + 0.5 * phase.cos());
    let _ = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
        .into_styled(PrimitiveStyle::with_fill(a.color.scaled(alpha)))
        .draw(c);
    let area = hub_area();
    let cx = area.top_left.x + area.size.width as i32 / 2;
    let cy = area.top_left.y + area.size.height as i32 / 2;
    let white = Rgba::rgb(255, 255, 255);
    match a.label.chars().count() {
        0..=3 => centred(c, &a.label, cx, cy - 5, MonoTextStyle::new(&FONT_6X10, white)),
        4 => centred(c, &a.label, cx, cy - 4, MonoTextStyle::new(&FONT_5X8, white)),
        // Longer labels go on the band above the hub, rows 23 to 28.
        _ => centred(c, &a.label, cx, 23, MonoTextStyle::new(&FONT_4X6, white)),
    }
}

pub fn draw(tiles: &Tiles, alerts: &[Alert], c: &mut Canvas, ctx: &Ctx) {
    if let Some(a) = active(alerts, ctx) {
        alert(a, c, ctx);
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
    let entries = [
        (Tile::TopLeft, &tiles.top_left),
        (Tile::TopRight, &tiles.top_right),
        (Tile::BottomLeft, &tiles.bottom_left),
        (Tile::BottomRight, &tiles.bottom_right),
    ];
    for (tile, entry) in entries {
        let palette = ctx.palette.with(&entry.colors);
        tiles::draw(&entry.spec, c, tile.rect(), ctx, &palette);
    }
    let palette = ctx.palette.with(&tiles.hub.colors);
    hub::draw(&tiles.hub.spec, c, ctx.data, ctx.frame, &palette);
}
