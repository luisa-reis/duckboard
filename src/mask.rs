//! The mask in front of the panel, and the areas the screen is split into.
//!
//! `2d-gaps.json` (WLED's gap-file format) marks the pixels the mask shows (1)
//! and hides (0). WLED drops the hidden ones itself, so a frame may draw
//! anywhere; the mask here is for previews. The screen is split into areas:
//! four 24x24 tiles at the corners and a 22x22 hub in the middle.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use anyhow::{bail, Context, Result};
use crate::palette::Rgba;
use embedded_graphics::{prelude::*, primitives::Rectangle};
use std::path::Path;

pub const TILE: Size = Size::new(24, 24);
pub const HUB: Size = Size::new(22, 22);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Tile {

    pub fn rect(self) -> Rectangle {
        let origin = match self {
            Tile::TopLeft => Point::new(2, 2),
            Tile::TopRight => Point::new(38, 2),
            Tile::BottomLeft => Point::new(2, 38),
            Tile::BottomRight => Point::new(38, 38),
        };
        Rectangle::new(origin, TILE)
    }
}

pub fn hub() -> Rectangle {
    Rectangle::new(Point::new(21, 21), HUB)
}

/// Which pixels the mask shows, row-major.
pub struct Mask {
    lit: Vec<bool>,
}

impl Mask {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading gap file {}", path.display()))?;
        let values: Vec<u8> = serde_json::from_str(&text)
            .with_context(|| format!("parsing gap file {}", path.display()))?;
        let want = (WIDTH * HEIGHT) as usize;
        if values.len() != want {
            bail!("gap file has {} entries, the panel has {want}", values.len());
        }
        Ok(Self { lit: values.iter().map(|&v| v == 1).collect() })
    }

    /// A mask that shows everything, for previews without a gap file.
    pub fn none() -> Self {
        Self { lit: vec![true; (WIDTH * HEIGHT) as usize] }
    }

    pub fn is_lit(&self, x: u32, y: u32) -> bool {
        self.lit[(y * WIDTH + x) as usize]
    }

    /// Writes the canvas as a PNG, scaled up, with the hidden pixels painted
    /// grey so the preview shows what the panel looks like behind the mask.
    pub fn preview_png(&self, canvas: &Canvas, scale: u32, path: &Path) -> Result<()> {
        let hidden = image::Rgb([0x30, 0x30, 0x30]);
        let mut img = image::RgbImage::new(WIDTH * scale, HEIGHT * scale);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let px = if self.is_lit(x, y) {
                    let c = canvas.get(x, y);
                    image::Rgb([c.r(), c.g(), c.b()])
                } else {
                    hidden
                };
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(x * scale + dx, y * scale + dy, px);
                    }
                }
            }
        }
        img.save(path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

/// Paints the mask's own shape onto a canvas, for checking the layout.
pub fn draw_outline(canvas: &mut Canvas, mask: &Mask, colour: Rgba) {
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if mask.is_lit(x, y) {
                Pixel(Point::new(x as i32, y as i32), colour).draw(canvas).unwrap();
            }
        }
    }
}
