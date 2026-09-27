//! The mask in front of the panel, for previews.
//!
//! `2d-gaps.json` (WLED's gap-file format) marks the pixels the mask shows (1)
//! and hides (0). WLED drops the hidden ones itself, so a frame may draw
//! anywhere; the mask here only greys the hidden ones out in a preview. The
//! regions the screen is split into are in the config's layouts.

use crate::canvas::Canvas;
use anyhow::{bail, Context, Result};
use crate::palette::Rgba;
use embedded_graphics::prelude::*;
use std::path::Path;

/// Which pixels the mask shows, row-major.
pub struct Mask {
    lit: Vec<bool>,
    size: Size,
}

impl Mask {
    /// The gap file for a panel of `size`; it must have a value per pixel.
    pub fn load(path: &Path, size: Size) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading gap file {}", path.display()))?;
        let values: Vec<u8> = serde_json::from_str(&text)
            .with_context(|| format!("parsing gap file {}", path.display()))?;
        let want = (size.width * size.height) as usize;
        if values.len() != want {
            bail!("gap file has {} entries, the {}x{} panel {want} pixels", values.len(), size.width, size.height);
        }
        Ok(Self { lit: values.iter().map(|&v| v == 1).collect(), size })
    }

    /// A mask that shows everything, for previews without a gap file.
    pub fn none(size: Size) -> Self {
        Self { lit: vec![true; (size.width * size.height) as usize], size }
    }

    pub fn is_lit(&self, x: u32, y: u32) -> bool {
        self.lit[(y * self.size.width + x) as usize]
    }

    /// Writes the canvas as a PNG, scaled up, with the hidden pixels painted
    /// grey so the preview shows what the panel looks like behind the mask.
    pub fn preview_png(&self, canvas: &Canvas, scale: u32, path: &Path) -> Result<()> {
        let hidden = image::Rgb([0x30, 0x30, 0x30]);
        let size = canvas.size();
        let mut img = image::RgbImage::new(size.width * scale, size.height * scale);
        for y in 0..size.height {
            for x in 0..size.width {
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
    for y in 0..mask.size.height {
        for x in 0..mask.size.width {
            if mask.is_lit(x, y) {
                Pixel(Point::new(x as i32, y as i32), colour).draw(canvas).unwrap();
            }
        }
    }
}
