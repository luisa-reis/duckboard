//! The hub, in its region (22x22 in the middle by default): album art
//! while something plays, as a record, the whole cover, or the cover with
//! faded corners, turning if asked; dimmed and still when paused; a slow
//! ripple when nothing is on.

use crate::canvas::Canvas;
use crate::config::{ArtShape, HubSpec};
use crate::data::Snapshot;
use crate::palette::{Palette, Rgba};
use embedded_graphics::{prelude::*, primitives::{Circle, PrimitiveStyle, Rectangle}};

/// Frames per full turn of the disc.
const TURN_FRAMES: u32 = 80;
/// Radius of the spindle hole, squared.
const HOLE_R2: f32 = 2.0 * 2.0;

pub fn draw(spec: &HubSpec, c: &mut Canvas, area: Rectangle, data: &Snapshot, frame: u32, p: &Palette) {
    let mut t = c.clipped(&area);
    let size = area.size;
    match spec {
        HubSpec::Blank => {}
        HubSpec::Media { spin, shape, paused_alpha, corner_alpha } => match data.media.as_ref().and_then(|m| m.art.as_ref().map(|a| (m, a))) {
            // Art is decoded at the hub's size; anything else is a stale
            // picture from before a change of region, so wait for the next.
            Some((m, a)) if a.rgb.len() == (size.width * size.height * 3) as usize => art(
                &mut t,
                area,
                &a.rgb,
                m.playing,
                *spin,
                *shape,
                *paused_alpha,
                *corner_alpha,
                frame,
            ),
            _ => ripple(&mut t, area, frame, p),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn art<D: DrawTarget<Color = Rgba>>(
    t: &mut D,
    area: Rectangle,
    rgb: &[u8],
    playing: bool,
    spin: bool,
    shape: ArtShape,
    paused_alpha: f32,
    corner_alpha: f32,
    frame: u32,
) {
    let (o, size) = (area.top_left, area.size);
    let (w, h) = (size.width as f32, size.height as f32);
    let (mid_x, mid_y) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
    // The disc is the largest circle that fits, centred.
    let r = w.min(h) / 2.0;
    let r_max2 = r * r;
    let turning = spin && playing;
    let angle = if turning { frame % TURN_FRAMES } else { 0 } as f32 * std::f32::consts::TAU / TURN_FRAMES as f32;
    let (sin, cos) = angle.sin_cos();
    let paused = if playing { 1.0 } else { paused_alpha };
    for y in 0..size.height {
        for x in 0..size.width {
            let dx = x as f32 - mid_x;
            let dy = y as f32 - mid_y;
            let r2 = dx * dx + dy * dy;
            let outside = r2 > r_max2;
            let alpha = match shape {
                ArtShape::Disc if outside || r2 < HOLE_R2 => continue,
                ArtShape::Faded if outside => paused * corner_alpha,
                _ => paused,
            };
            // Rotate the sample point the other way round the centre.
            let sx = (mid_x + dx * cos + dy * sin).round().clamp(0.0, w - 1.0) as usize;
            let sy = (mid_y - dx * sin + dy * cos).round().clamp(0.0, h - 1.0) as usize;
            let i = (sy * size.width as usize + sx) * 3;
            let px = Rgba::rgb(rgb[i], rgb[i + 1], rgb[i + 2]).scaled(alpha);
            let _ = Pixel(o + Point::new(x as i32, y as i32), px).draw(t);
        }
    }
}

/// A ring growing from the centre every few seconds and fading as it goes,
/// in the track colour, faint enough to ignore.
fn ripple<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, frame: u32, p: &Palette) {
    let (o, size) = (area.top_left, area.size);
    // Steps out to the largest circle that fits.
    let steps = size.width.min(size.height) / 2 + 1;
    let step = (frame / 4) % steps;
    let d = step * 2 + 1;
    let off = Point::new((size.width as i32 - d as i32) / 2, (size.height as i32 - d as i32) / 2);
    let _ = Circle::new(o + off, d)
        .into_styled(PrimitiveStyle::with_stroke(p.track.scaled(1.0 - step as f32 / steps as f32), 1))
        .draw(t);
    let (cx, cy) = (size.width as i32 / 2 - 1, size.height as i32 / 2 - 1);
    for (x, y) in [(cx, cy), (cx + 1, cy + 1), (cx, cy + 1), (cx + 1, cy)] {
        let _ = Pixel(o + Point::new(x, y), p.track).draw(t);
    }
}
