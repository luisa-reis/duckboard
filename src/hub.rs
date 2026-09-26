//! The 22x22 hub in the middle of the screen: album art while something
//! plays, as a record, the whole cover, or the cover with faded corners,
//! turning if asked; dimmed and still when paused; a slow ripple when
//! nothing is on.

use crate::canvas::Canvas;
use crate::config::{ArtShape, HubSpec};
use crate::data::Snapshot;
use crate::mask::{hub, HUB};
use crate::palette::{Palette, Rgba};
use embedded_graphics::{prelude::*, primitives::{Circle, PrimitiveStyle}};

/// Frames per full turn of the disc.
const TURN_FRAMES: u32 = 80;
/// Radius of the spindle hole, squared.
const HOLE_R2: f32 = 2.0 * 2.0;

pub fn draw(spec: &HubSpec, c: &mut Canvas, data: &Snapshot, frame: u32, p: &Palette) {
    let area = hub();
    let mut t = c.clipped(&area);
    match spec {
        HubSpec::Blank => {}
        HubSpec::Media { spin, shape, paused_alpha, corner_alpha } => match &data.media {
            Some(m) if m.art.is_some() => art(
                &mut t,
                area.top_left,
                &m.art.as_ref().unwrap().rgb,
                m.playing,
                *spin,
                *shape,
                *paused_alpha,
                *corner_alpha,
                frame,
            ),
            _ => ripple(&mut t, area.top_left, frame, p),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn art<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    o: Point,
    rgb: &[u8],
    playing: bool,
    spin: bool,
    shape: ArtShape,
    paused_alpha: f32,
    corner_alpha: f32,
    frame: u32,
) {
    let w = HUB.width as f32;
    let mid = (w - 1.0) / 2.0;
    let r_max2 = (w / 2.0) * (w / 2.0);
    let turning = spin && playing;
    let angle = if turning { frame % TURN_FRAMES } else { 0 } as f32 * std::f32::consts::TAU / TURN_FRAMES as f32;
    let (sin, cos) = angle.sin_cos();
    let paused = if playing { 1.0 } else { paused_alpha };
    for y in 0..HUB.height {
        for x in 0..HUB.width {
            let dx = x as f32 - mid;
            let dy = y as f32 - mid;
            let r2 = dx * dx + dy * dy;
            let outside = r2 > r_max2;
            let alpha = match shape {
                ArtShape::Disc if outside || r2 < HOLE_R2 => continue,
                ArtShape::Faded if outside => paused * corner_alpha,
                _ => paused,
            };
            // Rotate the sample point the other way round the centre.
            let sx = (mid + dx * cos + dy * sin).round().clamp(0.0, w - 1.0) as usize;
            let sy = (mid - dx * sin + dy * cos).round().clamp(0.0, w - 1.0) as usize;
            let i = (sy * HUB.width as usize + sx) * 3;
            let px = Rgba::rgb(rgb[i], rgb[i + 1], rgb[i + 2]).scaled(alpha);
            let _ = Pixel(o + Point::new(x as i32, y as i32), px).draw(t);
        }
    }
}

/// A ring growing from the centre every few seconds and fading as it goes,
/// in the track colour, faint enough to ignore.
fn ripple<D: DrawTarget<Color = Rgba>>(t: &mut D, o: Point, frame: u32, p: &Palette) {
    let step = (frame / 4) % 12;
    let d = step * 2 + 1;
    let off = (HUB.width as i32 - d as i32) / 2;
    let _ = Circle::new(o + Point::new(off, off), d)
        .into_styled(PrimitiveStyle::with_stroke(p.track.scaled(1.0 - step as f32 / 12.0), 1))
        .draw(t);
    for (x, y) in [(10, 10), (11, 11), (10, 11), (11, 10)] {
        let _ = Pixel(o + Point::new(x, y), p.track).draw(t);
    }
}
