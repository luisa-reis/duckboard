//! Weather icons, drawn with primitives into a 12x12 box so they stay
//! consistent and take their colours from the palette. `o` is the box's
//! top-left corner.

use crate::palette::{Palette, Rgba, BLACK};
use crate::weather::Sky;
use embedded_graphics::{
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
};

pub const SIZE: u32 = 12;

fn fill<D: DrawTarget<Color = Rgba>>(t: &mut D, r: Rectangle, c: Rgba) {
    let _ = r.into_styled(PrimitiveStyle::with_fill(c)).draw(t);
}

fn disc<D: DrawTarget<Color = Rgba>>(t: &mut D, top_left: Point, d: u32, c: Rgba) {
    let _ = Circle::new(top_left, d).into_styled(PrimitiveStyle::with_fill(c)).draw(t);
}

fn dot<D: DrawTarget<Color = Rgba>>(t: &mut D, p: Point, c: Rgba) {
    let _ = Pixel(p, c).draw(t);
}

/// A sun of diameter 6 with eight rays, centred on `c`.
fn sun<D: DrawTarget<Color = Rgba>>(t: &mut D, c: Point, colour: Rgba) {
    disc(t, c - Point::new(2, 2), 5, colour);
    for (dx, dy) in [(0, -4), (0, 4), (-4, 0), (4, 0), (-3, -3), (3, -3), (-3, 3), (3, 3)] {
        dot(t, c + Point::new(dx, dy), colour);
    }
}

fn moon<D: DrawTarget<Color = Rgba>>(t: &mut D, c: Point, colour: Rgba) {
    disc(t, c - Point::new(3, 3), 7, colour);
    disc(t, c - Point::new(1, 4), 6, BLACK);
}

/// A cloud filling the lower two thirds of the box.
fn cloud<D: DrawTarget<Color = Rgba>>(t: &mut D, o: Point, colour: Rgba) {
    disc(t, o + Point::new(1, 5), 5, colour);
    disc(t, o + Point::new(4, 3), 6, colour);
    disc(t, o + Point::new(7, 5), 5, colour);
    fill(t, Rectangle::new(o + Point::new(2, 7), Size::new(8, 3)), colour);
}

pub fn draw<D: DrawTarget<Color = Rgba>>(t: &mut D, sky: Sky, is_day: bool, o: Point, p: &Palette) {
    let centre = o + Point::new(6, 6);
    match sky {
        Sky::Clear if is_day => sun(t, centre, p.sun),
        Sky::Clear => moon(t, centre, p.moon),
        Sky::PartlyCloudy => {
            if is_day {
                sun(t, o + Point::new(4, 4), p.sun);
            } else {
                moon(t, o + Point::new(4, 4), p.moon);
            }
            cloud(t, o + Point::new(1, 2), p.cloud);
        }
        Sky::Cloudy => cloud(t, o + Point::new(0, 1), p.cloud),
        Sky::Fog => {
            for y in [2, 5, 8] {
                let _ = Line::new(o + Point::new(1, y), o + Point::new(10, y))
                    .into_styled(PrimitiveStyle::with_stroke(p.fog, 1))
                    .draw(t);
            }
        }
        Sky::Rain => {
            cloud(t, o + Point::new(0, -1), p.cloud);
            for x in [2, 5, 8] {
                let _ = Line::new(o + Point::new(x, 9), o + Point::new(x - 1, 11))
                    .into_styled(PrimitiveStyle::with_stroke(p.rain, 1))
                    .draw(t);
            }
        }
        Sky::Snow => {
            cloud(t, o + Point::new(0, -1), p.cloud);
            for (x, y) in [(2, 9), (5, 10), (8, 9), (3, 11), (7, 11)] {
                dot(t, o + Point::new(x, y), p.snow);
            }
        }
        Sky::Storm => {
            cloud(t, o + Point::new(0, -1), p.storm_cloud);
            for (x, y) in [(6, 7), (5, 8), (6, 9), (5, 10), (4, 11)] {
                dot(t, o + Point::new(x, y), p.sun);
            }
        }
    }
}
