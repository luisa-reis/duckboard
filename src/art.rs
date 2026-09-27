//! The picture tiles. `art`: the album art as a record, the whole cover, or
//! the cover with faded corners, turning if asked, dimmed and still when
//! paused, and a slow ripple (or nothing) while there is none. `picture`:
//! the `[frame]` picture due. Both fill their area and blend over what is
//! drawn under them at their alpha.

use crate::canvas::Canvas;
use crate::config::{ArtShape, Idle};
use crate::data::Snapshot;
use crate::palette::{Palette, Rgba};
use crate::picture::Scaled;
use crate::tiles::Ctx;
use embedded_graphics::{prelude::*, primitives::{Circle, PrimitiveStyle, Rectangle}};

/// One full turn of a spinning disc, in seconds.
const TURN_SECONDS: f32 = 8.0;
/// How long each step of the idle ripple lasts, in milliseconds.
const RIPPLE_STEP_MS: u64 = 400;
/// Radius of the spindle hole, squared.
const HOLE_R2: f32 = 2.0 * 2.0;

/// How an art tile shows the cover.
pub struct Style {
    pub shape: ArtShape,
    pub spin: bool,
    pub paused_alpha: f32,
    pub corner_alpha: f32,
    pub alpha: f32,
    pub idle: Idle,
}

pub fn draw(style: &Style, c: &mut Canvas, area: Rectangle, ctx: &Ctx, p: &Palette) {
    let data: &Snapshot = ctx.data;
    let mut t = c.clipped(&area);
    // Art is decoded at every size it shows at; a picture without this one
    // is from before a change of layout, so wait for the next.
    match data.media.as_ref().and_then(|m| Some((m, m.art.as_ref()?.scaled.at(area.size)?))) {
        Some((m, rgb)) => art(&mut t, area, rgb, m.playing, style, ctx),
        None if style.idle == Idle::Ripple => ripple(&mut t, area, ctx, p),
        None => {}
    }
}

/// The `[frame]` picture due, filling the area at `alpha`.
pub fn picture(c: &mut Canvas, area: Rectangle, picture: Option<&Scaled>, alpha: f32) {
    let Some(rgb) = picture.and_then(|p| p.at(area.size)) else { return };
    let mut t = c.clipped(&area);
    let pixels = area.points().enumerate().map(|(i, q)| {
        let i = i * 3;
        Pixel(q, Rgba::rgb(rgb[i], rgb[i + 1], rgb[i + 2]).scaled(alpha))
    });
    let _ = t.draw_iter(pixels);
}

fn art<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, rgb: &[u8], playing: bool, style: &Style, ctx: &Ctx) {
    let (o, size) = (area.top_left, area.size);
    let (w, h) = (size.width as f32, size.height as f32);
    let (mid_x, mid_y) = ((w - 1.0) / 2.0, (h - 1.0) / 2.0);
    // The disc is the largest circle that fits, centred.
    let r = w.min(h) / 2.0;
    let r_max2 = r * r;
    let turning = style.spin && playing;
    // Frames per turn at this rate: the frame within the turn over that is
    // how far round it is.
    let per_turn = ((TURN_SECONDS * ctx.fps as f32).round() as u32).max(1);
    let angle = if turning { ctx.frame % per_turn } else { 0 } as f32 * std::f32::consts::TAU / per_turn as f32;
    let (sin, cos) = angle.sin_cos();
    let paused = if playing { 1.0 } else { style.paused_alpha };
    for y in 0..size.height {
        for x in 0..size.width {
            let dx = x as f32 - mid_x;
            let dy = y as f32 - mid_y;
            let r2 = dx * dx + dy * dy;
            let outside = r2 > r_max2;
            let alpha = match style.shape {
                ArtShape::Disc if outside || r2 < HOLE_R2 => continue,
                ArtShape::Faded if outside => paused * style.corner_alpha,
                _ => paused,
            } * style.alpha;
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
fn ripple<D: DrawTarget<Color = Rgba>>(t: &mut D, area: Rectangle, ctx: &Ctx, p: &Palette) {
    let (o, size) = (area.top_left, area.size);
    // Steps out to the largest circle that fits.
    let steps = size.width.min(size.height) / 2 + 1;
    let step = (ctx.elapsed_ms() / RIPPLE_STEP_MS % steps as u64) as u32;
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
