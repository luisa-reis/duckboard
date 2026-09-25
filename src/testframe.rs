//! The test frame: a border so the edges are visible, RGB and white bars to
//! check colour order and brightness, a grey ramp, a frame counter and a
//! bouncing dot so a stuck stream is obvious.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

pub fn draw(c: &mut Canvas, frame: u32) {
    c.clear(Rgb888::BLACK).unwrap();

    Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(64, 64, 64), 1))
        .draw(c)
        .unwrap();

    let bars = [Rgb888::RED, Rgb888::GREEN, Rgb888::BLUE, Rgb888::WHITE];
    for (i, colour) in bars.iter().enumerate() {
        Rectangle::new(Point::new(4 + i as i32 * 14, 4), Size::new(14, 8))
            .into_styled(PrimitiveStyle::with_fill(*colour))
            .draw(c)
            .unwrap();
    }

    for x in 0..56i32 {
        let v = (x * 255 / 55) as u8;
        Line::new(Point::new(4 + x, 14), Point::new(4 + x, 17))
            .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(v, v, v), 1))
            .draw(c)
            .unwrap();
    }

    let big = MonoTextStyle::new(&FONT_6X10, Rgb888::new(255, 200, 0));
    Text::with_alignment("DDP", Point::new(32, 31), big, Alignment::Center)
        .draw(c)
        .unwrap();

    let small = MonoTextStyle::new(&FONT_4X6, Rgb888::new(0, 200, 255));
    Text::with_alignment(&format!("{:05}", frame), Point::new(32, 41), small, Alignment::Center)
        .draw(c)
        .unwrap();

    let lane = (WIDTH - 12) as i32;
    let t = frame as i32 % (2 * lane);
    let x = if t < lane { t } else { 2 * lane - t };
    Circle::new(Point::new(4 + x, 50), 5)
        .into_styled(PrimitiveStyle::with_fill(Rgb888::MAGENTA))
        .draw(c)
        .unwrap();
}
