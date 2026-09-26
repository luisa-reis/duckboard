//! Puts the tiles in the corners and the hub in the middle.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::{Background, Tiles};
use crate::hub;
use crate::mask::Tile;
use crate::palette::BLACK;
use crate::tiles::{self, Ctx};
use embedded_graphics::prelude::*;

pub fn draw(tiles: &Tiles, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
    if let Background::Media { brightness } = tiles.background {
        if let Some(art) = ctx.data.media.as_ref().and_then(|m| m.art.as_ref()) {
            // Paused playback dims the background further, like the hub.
            let dim = brightness * if ctx.data.media.as_ref().is_some_and(|m| m.playing) { 1.0 } else { 0.4 };
            for i in 0..(WIDTH * HEIGHT) as usize {
                let p = &art.full[i * 3..i * 3 + 3];
                c.px[i * 3] = (p[0] as f32 * dim) as u8;
                c.px[i * 3 + 1] = (p[1] as f32 * dim) as u8;
                c.px[i * 3 + 2] = (p[2] as f32 * dim) as u8;
            }
        }
    }
    let specs = [
        (Tile::TopLeft, &tiles.top_left),
        (Tile::TopRight, &tiles.top_right),
        (Tile::BottomLeft, &tiles.bottom_left),
        (Tile::BottomRight, &tiles.bottom_right),
    ];
    for (tile, spec) in specs {
        tiles::draw(spec, c, tile.rect(), ctx);
    }
    hub::draw(&tiles.hub, c, ctx.data, ctx.frame);
}
