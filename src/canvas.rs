//! The canvas that embedded-graphics draws into, the panel's size. Its bytes
//! are row-major RGB, which is already the DDP wire format; a pixel drawn
//! with alpha is blended over what is there.

use crate::palette::Rgba;
use embedded_graphics::{pixelcolor::Rgb888, prelude::*};
use std::convert::Infallible;

/// The panel's size unless the configuration says otherwise.
pub const DEFAULT_WIDTH: u32 = 64;
pub const DEFAULT_HEIGHT: u32 = 64;

pub struct Canvas {
    pub px: Vec<u8>,
    width: u32,
    height: u32,
}

impl Canvas {
    pub fn new(size: Size) -> Self {
        Self { px: vec![0; (size.width * size.height * 3) as usize], width: size.width, height: size.height }
    }

    pub fn get(&self, x: u32, y: u32) -> Rgb888 {
        let i = ((y * self.width + x) * 3) as usize;
        Rgb888::new(self.px[i], self.px[i + 1], self.px[i + 2])
    }
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for Canvas {
    type Color = Rgba;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let (w, h) = (self.width, self.height);
        for Pixel(p, c) in pixels {
            if p.x < 0 || p.y < 0 || p.x >= w as i32 || p.y >= h as i32 || c.a == 0 {
                continue;
            }
            let i = ((p.y as u32 * w + p.x as u32) * 3) as usize;
            if c.a == 255 {
                self.px[i] = c.r;
                self.px[i + 1] = c.g;
                self.px[i + 2] = c.b;
            } else {
                let a = c.a as u32;
                let mix = |src: u8, dst: u8| ((src as u32 * a + dst as u32 * (255 - a) + 127) / 255) as u8;
                self.px[i] = mix(c.r, self.px[i]);
                self.px[i + 1] = mix(c.g, self.px[i + 1]);
                self.px[i + 2] = mix(c.b, self.px[i + 2]);
            }
        }
        Ok(())
    }
}
