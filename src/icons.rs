//! Weather icons, drawn with primitives into a 12x12 box so they stay
//! consistent and can be recoloured. `o` is the box's top-left corner.

use crate::palette::*;
use crate::weather::Sky;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
};

pub const SIZE: u32 = 12;

const CLOUD: Rgb888 = Rgb888::new(200, 200, 210);
const STORM_CLOUD: Rgb888 = Rgb888::new(120, 120, 140);
const RAIN: Rgb888 = Rgb888::new(60, 140, 255);
const MOON: Rgb888 = Rgb888::new(220, 220, 180);

fn fill<D: DrawTarget<Color = Rgb888>>(t: &mut D, r: Rectangle, c: Rgb888) {
    let _ = r.into_styled(PrimitiveStyle::with_fill(c)).draw(t);
}

fn disc<D: DrawTarget<Color = Rgb888>>(t: &mut D, top_left: Point, d: u32, c: Rgb888) {
    let _ = Circle::new(top_left, d).into_styled(PrimitiveStyle::with_fill(c)).draw(t);
}

fn dot<D: DrawTarget<Color = Rgb888>>(t: &mut D, p: Point, c: Rgb888) {
    let _ = Pixel(p, c).draw(t);
}

/// A sun of diameter 6 with eight rays, centred on `c`.
fn sun<D: DrawTarget<Color = Rgb888>>(t: &mut D, c: Point, colour: Rgb888) {
    disc(t, c - Point::new(2, 2), 5, colour);
    for (dx, dy) in [(0, -4), (0, 4), (-4, 0), (4, 0), (-3, -3), (3, -3), (-3, 3), (3, 3)] {
        dot(t, c + Point::new(dx, dy), colour);
    }
}

fn moon<D: DrawTarget<Color = Rgb888>>(t: &mut D, c: Point) {
    disc(t, c - Point::new(3, 3), 7, MOON);
    disc(t, c - Point::new(1, 4), 6, BLACK);
}

/// A cloud filling the lower two thirds of the box.
fn cloud<D: DrawTarget<Color = Rgb888>>(t: &mut D, o: Point, colour: Rgb888) {
    disc(t, o + Point::new(1, 5), 5, colour);
    disc(t, o + Point::new(4, 3), 6, colour);
    disc(t, o + Point::new(7, 5), 5, colour);
    fill(t, Rectangle::new(o + Point::new(2, 7), Size::new(8, 3)), colour);
}

pub fn draw<D: DrawTarget<Color = Rgb888>>(t: &mut D, sky: Sky, is_day: bool, o: Point) {
    let centre = o + Point::new(6, 6);
    match sky {
        Sky::Clear if is_day => sun(t, centre, AMBER),
        Sky::Clear => moon(t, centre),
        Sky::PartlyCloudy => {
            if is_day {
                sun(t, o + Point::new(4, 4), AMBER);
            } else {
                moon(t, o + Point::new(4, 4));
            }
            cloud(t, o + Point::new(1, 2), CLOUD);
        }
        Sky::Cloudy => cloud(t, o + Point::new(0, 1), CLOUD),
        Sky::Fog => {
            for y in [2, 5, 8] {
                let _ = Line::new(o + Point::new(1, y), o + Point::new(10, y))
                    .into_styled(PrimitiveStyle::with_stroke(GREY, 1))
                    .draw(t);
            }
        }
        Sky::Rain => {
            cloud(t, o + Point::new(0, -1), CLOUD);
            for x in [2, 5, 8] {
                let _ = Line::new(o + Point::new(x, 9), o + Point::new(x - 1, 11))
                    .into_styled(PrimitiveStyle::with_stroke(RAIN, 1))
                    .draw(t);
            }
        }
        Sky::Snow => {
            cloud(t, o + Point::new(0, -1), CLOUD);
            for (x, y) in [(2, 9), (5, 10), (8, 9), (3, 11), (7, 11)] {
                dot(t, o + Point::new(x, y), WHITE);
            }
        }
        Sky::Storm => {
            cloud(t, o + Point::new(0, -1), STORM_CLOUD);
            for (x, y) in [(6, 7), (5, 8), (6, 9), (5, 10), (4, 11)] {
                dot(t, o + Point::new(x, y), AMBER);
            }
        }
    }
}
