//! Puts the tiles in the corners and the hub in the middle, over the
//! background.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::{Background, Tiles};
use crate::hub;
use crate::mask::Tile;
use crate::palette::BLACK;
use crate::tiles::{self, Ctx};
use embedded_graphics::prelude::*;

pub fn draw(tiles: &Tiles, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
    if let Some(Background::Media { alpha }) = tiles.background {
        if let Some(art) = ctx.data.media.as_ref().and_then(|m| m.art.as_ref()) {
            // The art over the black panel at `alpha`, and nothing else.
            for i in 0..(WIDTH * HEIGHT) as usize {
                let p = &art.full[i * 3..i * 3 + 3];
                c.px[i * 3] = (p[0] as f32 * alpha) as u8;
                c.px[i * 3 + 1] = (p[1] as f32 * alpha) as u8;
                c.px[i * 3 + 2] = (p[2] as f32 * alpha) as u8;
            }
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
