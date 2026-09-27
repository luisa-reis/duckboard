//! The test frame: a border so the edges are visible, RGB and white bars to
//! check colour order and brightness, a grey ramp, a frame counter and a
//! bouncing dot so a stuck stream is obvious.

use crate::canvas::Canvas;
use crate::palette::{Rgba, BLACK};
use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

pub fn draw(c: &mut Canvas, frame: u32) {
    c.clear(BLACK).unwrap();
    let size = c.size();
    let cx = size.width as i32 / 2;

    Rectangle::new(Point::zero(), size)
        .into_styled(PrimitiveStyle::with_stroke(Rgba::rgb(64, 64, 64), 1))
        .draw(c)
        .unwrap();

    let bars = [Rgba::rgb(255, 0, 0), Rgba::rgb(0, 255, 0), Rgba::rgb(0, 0, 255), Rgba::rgb(255, 255, 255)];
    for (i, colour) in bars.iter().enumerate() {
        Rectangle::new(Point::new(4 + i as i32 * 14, 4), Size::new(14, 8))
            .into_styled(PrimitiveStyle::with_fill(*colour))
            .draw(c)
            .unwrap();
    }

    for x in 0..56i32 {
        let v = (x * 255 / 55) as u8;
        Line::new(Point::new(4 + x, 14), Point::new(4 + x, 17))
            .into_styled(PrimitiveStyle::with_stroke(Rgba::rgb(v, v, v), 1))
            .draw(c)
            .unwrap();
    }

    let big = MonoTextStyle::new(&FONT_6X10, Rgba::rgb(255, 200, 0));
    Text::with_alignment("DDP", Point::new(cx, 31), big, Alignment::Center)
        .draw(c)
        .unwrap();

    let small = MonoTextStyle::new(&FONT_4X6, Rgba::rgb(0, 200, 255));
    Text::with_alignment(&format!("{:05}", frame), Point::new(cx, 41), small, Alignment::Center)
        .draw(c)
        .unwrap();

    let lane = (size.width as i32 - 12).max(1);
    let t = frame as i32 % (2 * lane);
    let x = if t < lane { t } else { 2 * lane - t };
    Circle::new(Point::new(4 + x, 50), 5)
        .into_styled(PrimitiveStyle::with_fill(Rgba::rgb(255, 0, 255)))
        .draw(c)
        .unwrap();
}
