//! The 64x64 RGB canvas that embedded-graphics draws into. Its bytes are
//! row-major RGB, which is already the DDP wire format.

use embedded_graphics::{pixelcolor::Rgb888, prelude::*};
use std::convert::Infallible;

pub const WIDTH: u32 = 64;
pub const HEIGHT: u32 = 64;

pub struct Canvas {
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new() -> Self {
        Self { px: vec![0; (WIDTH * HEIGHT * 3) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> Rgb888 {
        let i = ((y * WIDTH + x) * 3) as usize;
        Rgb888::new(self.px[i], self.px[i + 1], self.px[i + 2])
    }
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Canvas {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x < 0 || p.y < 0 || p.x >= WIDTH as i32 || p.y >= HEIGHT as i32 {
                continue;
            }
            let i = ((p.y as u32 * WIDTH + p.x as u32) * 3) as usize;
            self.px[i] = c.r();
            self.px[i + 1] = c.g();
            self.px[i + 2] = c.b();
        }
        Ok(())
    }
}
