//! Puts the tiles in the corners and the hub in the middle.

use crate::canvas::Canvas;
use crate::config::Tiles;
use crate::hub;
use crate::mask::Tile;
use crate::palette::BLACK;
use crate::tiles::{self, Ctx};
use embedded_graphics::prelude::*;

pub fn draw(tiles: &Tiles, c: &mut Canvas, ctx: &Ctx) {
    c.clear(BLACK).unwrap();
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
