//! The mask in front of the panel, for previews.
//!
//! `2d-gaps.json` (WLED's gap-file format) marks the pixels the mask shows (1)
//! and hides (0). WLED drops the hidden ones itself, so a frame may draw
//! anywhere; the mask here only greys the hidden ones out in a preview. The
//! regions the screen is split into are in the config's layouts.

use crate::canvas::Canvas;
use anyhow::{bail, Context, Result};
use crate::palette::Rgba;
use embedded_graphics::prelude::*;
use std::path::Path;

/// Which pixels the mask shows, row-major.
pub struct Mask {
    lit: Vec<bool>,
    size: Size,
}

impl Mask {
    /// The gap file for a panel of `size`; it must have a value per pixel.
    pub fn load(path: &Path, size: Size) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading gap file {}", path.display()))?;
        let values: Vec<u8> = serde_json::from_str(&text)
            .with_context(|| format!("parsing gap file {}", path.display()))?;
        let want = (size.width * size.height) as usize;
        if values.len() != want {
            bail!("gap file has {} entries, the {}x{} panel {want} pixels", values.len(), size.width, size.height);
        }
        Ok(Self { lit: values.iter().map(|&v| v == 1).collect(), size })
    }

    /// A mask that shows everything, for previews without a gap file.
    pub fn none(size: Size) -> Self {
        Self { lit: vec![true; (size.width * size.height) as usize], size }
    }

    pub fn is_lit(&self, x: u32, y: u32) -> bool {
        self.lit[(y * self.size.width + x) as usize]
    }

    /// Writes the canvas as a PNG, scaled up, with the hidden pixels painted
    /// grey so the preview shows what the panel looks like behind the mask.
    pub fn preview_png(&self, canvas: &Canvas, scale: u32, path: &Path) -> Result<()> {
        self.preview(canvas, scale).save(path).with_context(|| format!("writing {}", path.display()))
    }

    /// Writes the canvases as an animated GIF at `fps`, looping, each as
    /// `preview_png` would draw it. A GIF has 256 colours a frame.
    pub fn preview_gif(&self, canvases: impl ExactSizeIterator<Item = Canvas>, scale: u32, fps: u32, path: &Path) -> Result<()> {
        // The gif crate itself: image's encoder draws every frame at the
        // corner, whatever position it is given.
        let (width, height) = (self.size.width * scale, self.size.height * scale);
        let size = |v: u32| u16::try_from(v).ok().with_context(|| format!("{width}×{height} is too large for a GIF"));
        let write = || -> Result<()> {
            let file = std::io::BufWriter::new(std::fs::File::create(path)?);
            let mut gif = gif::Encoder::new(file, size(width)?, size(height)?, &[])?;
            gif.set_repeat(gif::Repeat::Infinite)?;
            // Frames shown so far: delays are in hundredths of a second, and
            // rounding each alone would drift at a rate that is not a tenth.
            let (mut shown, fps) = (0u32, fps.max(1));
            for part in self.parts(canvases, scale) {
                let (w, h) = (size(part.image.width())?, size(part.image.height())?);
                let mut frame = gif::Frame::from_rgb_speed(w, h, part.image.as_raw(), 10);
                (frame.left, frame.top) = (size(part.x)?, size(part.y)?);
                let (from, to) = (100 * shown / fps, 100 * (shown + part.frames) / fps);
                frame.delay = (to - from).clamp(1, u16::MAX as u32) as u16;
                shown += part.frames;
                frame.dispose = gif::DisposalMethod::Keep;
                gif.write_frame(&frame)?;
            }
            Ok(())
        };
        write().with_context(|| format!("writing {}", path.display()))
    }

    /// Writes the canvases as an animated PNG (APNG), like `preview_gif` but
    /// in full colour. Viewers without APNG show the first frame.
    pub fn preview_apng(&self, canvases: impl ExactSizeIterator<Item = Canvas>, scale: u32, fps: u32, path: &Path) -> Result<()> {
        let parts = self.parts(canvases, scale);
        let write = || -> Result<(), png::EncodingError> {
            let file = std::fs::File::create(path)?;
            let mut png = png::Encoder::new(std::io::BufWriter::new(file), self.size.width * scale, self.size.height * scale);
            png.set_color(png::ColorType::Rgb);
            png.set_depth(png::BitDepth::Eight);
            png.set_compression(png::Compression::High);
            png.set_animated(parts.len() as u32, 0)?;
            let mut png = png.write_header()?;
            for part in &parts {
                // Each is checked against the other as it stands, so the
                // size is set at the corner, where any size fits.
                png.set_frame_position(0, 0)?;
                png.set_frame_dimension(part.image.width(), part.image.height())?;
                png.set_frame_position(part.x, part.y)?;
                png.set_frame_delay(part.frames.min(u16::MAX as u32) as u16, fps.clamp(1, u16::MAX as u32) as u16)?;
                png.write_image_data(part.image.as_raw())?;
            }
            png.finish()
        };
        write().with_context(|| format!("writing {}", path.display()))
    }

    /// The canvases as an animation's frames, small to store: each is only
    /// the rectangle that differs from the frame before, laid over it, and
    /// frames that do not differ at all add to the time of the one before.
    /// The first is whole.
    fn parts(&self, canvases: impl Iterator<Item = Canvas>, scale: u32) -> Vec<Part> {
        let mut parts: Vec<Part> = Vec::new();
        let mut shown: Option<image::RgbImage> = None;
        for canvas in canvases {
            let img = self.preview(&canvas, scale);
            let Some(before) = &shown else {
                parts.push(Part { x: 0, y: 0, image: img.clone(), frames: 1 });
                shown = Some(img);
                continue;
            };
            // The rectangle around every pixel that changed.
            let changed = img.enumerate_pixels().filter(|(x, y, px)| before.get_pixel(*x, *y) != *px);
            let bounds = changed.fold(None, |b: Option<(u32, u32, u32, u32)>, (x, y, _)| match b {
                None => Some((x, y, x, y)),
                Some((x0, y0, x1, y1)) => Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y))),
            });
            match bounds {
                None => parts.last_mut().expect("a first frame").frames += 1,
                Some((x0, y0, x1, y1)) => {
                    let image = image::imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
                    parts.push(Part { x: x0, y: y0, image, frames: 1 });
                    shown = Some(img);
                }
            }
        }
        parts
    }

    /// The canvas scaled up, the hidden pixels grey.
    fn preview(&self, canvas: &Canvas, scale: u32) -> image::RgbImage {
        let hidden = image::Rgb([0x30, 0x30, 0x30]);
        let size = canvas.size();
        let mut img = image::RgbImage::new(size.width * scale, size.height * scale);
        for y in 0..size.height {
            for x in 0..size.width {
                let px = if self.is_lit(x, y) {
                    let c = canvas.get(x, y);
                    image::Rgb([c.r(), c.g(), c.b()])
                } else {
                    hidden
                };
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(x * scale + dx, y * scale + dy, px);
                    }
                }
            }
        }
        img
    }
}

/// Paints the mask's own shape onto a canvas, for checking the layout.
pub fn draw_outline(canvas: &mut Canvas, mask: &Mask, colour: Rgba) {
    for y in 0..mask.size.height {
        for x in 0..mask.size.width {
            if mask.is_lit(x, y) {
                Pixel(Point::new(x as i32, y as i32), colour).draw(canvas).unwrap();
            }
        }
    }
}

/// A frame of an animation as it is stored: what to lay over the frame
/// before, where, and for how many of the frames it stands for.
struct Part {
    x: u32,
    y: u32,
    image: image::RgbImage,
    frames: u32,
}

#[cfg(test)]
mod animation_tests {
    use super::*;

    /// A 4×4 canvas, black but for a white pixel where one is given.
    fn canvas(lit: Option<(u32, u32)>) -> Canvas {
        let mut c = Canvas::new(Size::new(4, 4));
        if let Some((x, y)) = lit {
            let at = ((y * 4 + x) * 3) as usize;
            c.px[at..at + 3].fill(255);
        }
        c
    }

    #[test]
    fn an_animation_stores_what_changed_and_for_how_long() {
        let mask = Mask::none(Size::new(4, 4));
        let frames = [canvas(None), canvas(None), canvas(Some((1, 2))), canvas(Some((1, 2))), canvas(Some((1, 2))), canvas(Some((3, 2)))];
        let parts = mask.parts(frames.into_iter(), 2);
        let seen: Vec<_> = parts.iter().map(|p| (p.x, p.y, p.image.width(), p.image.height(), p.frames)).collect();
        // The first whole, then the pixel that lit, then the rectangle
        // around the one that went dark and the one that lit.
        assert_eq!(seen, [(0, 0, 8, 8, 2), (2, 4, 2, 2, 3), (2, 4, 6, 2, 1)]);
        assert_eq!(parts.iter().map(|p| p.frames).sum::<u32>(), 6, "every frame is counted");
    }

    #[test]
    fn animations_written_small_read_back_as_drawn() {
        let mask = Mask::none(Size::new(4, 4));
        let frames = || [canvas(None), canvas(Some((1, 2))), canvas(Some((1, 2))), canvas(Some((3, 0)))].into_iter();
        let dir = std::env::temp_dir().join(format!("panel-ddp-anim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.gif");
        mask.preview_gif(frames(), 2, 10, &path).unwrap();
        // Laid over each other in turn, the stored frames are the drawn ones.
        let mut gif = gif::DecodeOptions::new();
        gif.set_color_output(gif::ColorOutput::RGBA);
        let mut gif = gif.read_info(std::fs::File::open(&path).unwrap()).unwrap();
        let mut screen = image::RgbaImage::new(8, 8);
        let mut shown = Vec::new();
        while let Some(frame) = gif.read_next_frame().unwrap() {
            let part = image::RgbaImage::from_raw(frame.width.into(), frame.height.into(), frame.buffer.to_vec()).unwrap();
            image::imageops::replace(&mut screen, &part, frame.left.into(), frame.top.into());
            for _ in 0..frame.delay / 10 {
                shown.push(image::DynamicImage::ImageRgba8(screen.clone()).into_rgb8());
            }
        }
        let drawn: Vec<_> = frames().map(|c| mask.preview(&c, 2)).collect();
        assert!(shown == drawn, "the GIF plays the frames it was given");
        mask.preview_apng(frames(), 2, 10, &dir.join("a.apng")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
