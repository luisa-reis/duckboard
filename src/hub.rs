//! The 22x22 hub in the middle of the screen: album art as a spinning disc
//! while something plays, dimmed and still when paused, and a slow ripple
//! when nothing is on.

use crate::canvas::Canvas;
use crate::config::HubSpec;
use crate::data::Snapshot;
use crate::mask::{hub, HUB};
use crate::palette::*;
use embedded_graphics::{pixelcolor::Rgb888, prelude::*, primitives::{Circle, PrimitiveStyle}};

/// Frames per full turn of the disc.
const TURN_FRAMES: u32 = 80;
/// Radius of the spindle hole, squared.
const HOLE_R2: f32 = 2.0 * 2.0;

pub fn draw(spec: &HubSpec, c: &mut Canvas, data: &Snapshot, frame: u32) {
    let area = hub();
    let mut t = c.clipped(&area);
    match spec {
        HubSpec::Blank => {}
        HubSpec::Media => match &data.media {
            Some(m) if m.art.is_some() => disc(&mut t, area.top_left, &m.art.as_ref().unwrap().rgb, m.playing, frame),
            _ => ripple(&mut t, area.top_left, frame),
        },
    }
}

fn disc<D: DrawTarget<Color = Rgb888>>(t: &mut D, o: Point, rgb: &[u8], playing: bool, frame: u32) {
    let w = HUB.width as f32;
    let mid = (w - 1.0) / 2.0;
    let r_max2 = (w / 2.0) * (w / 2.0);
    let angle = if playing { frame % TURN_FRAMES } else { 0 } as f32 * std::f32::consts::TAU / TURN_FRAMES as f32;
    let (sin, cos) = angle.sin_cos();
    let dim = if playing { 1.0 } else { 0.4 };
    for y in 0..HUB.height {
        for x in 0..HUB.width {
            let dx = x as f32 - mid;
            let dy = y as f32 - mid;
            let r2 = dx * dx + dy * dy;
            if r2 > r_max2 || r2 < HOLE_R2 {
                continue;
            }
            // Rotate the sample point the other way round the centre.
            let sx = (mid + dx * cos + dy * sin).round().clamp(0.0, w - 1.0) as usize;
            let sy = (mid - dx * sin + dy * cos).round().clamp(0.0, w - 1.0) as usize;
            let i = (sy * HUB.width as usize + sx) * 3;
            let px = Rgb888::new(
                (rgb[i] as f32 * dim) as u8,
                (rgb[i + 1] as f32 * dim) as u8,
                (rgb[i + 2] as f32 * dim) as u8,
            );
            let _ = Pixel(o + Point::new(x as i32, y as i32), px).draw(t);
        }
    }
}

/// A ring growing from the centre every few seconds, faint enough to ignore.
fn ripple<D: DrawTarget<Color = Rgb888>>(t: &mut D, o: Point, frame: u32) {
    let step = (frame / 4) % 12;
    let d = step * 2 + 1;
    let off = (HUB.width as i32 - d as i32) / 2;
    let fade = 45 - step as u8 * 3;
    let _ = Circle::new(o + Point::new(off, off), d)
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(fade, fade, fade), 1))
        .draw(t);
    let _ = Pixel(o + Point::new(10, 10), DIM).draw(t);
    let _ = Pixel(o + Point::new(11, 11), DIM).draw(t);
    let _ = Pixel(o + Point::new(10, 11), DIM).draw(t);
    let _ = Pixel(o + Point::new(11, 10), DIM).draw(t);
}
