//! Picture frame mode: the pictures in a folder, one after another, filling
//! the panel, nothing else drawn. Each is decoded once,
//! cropped to square, scaled to the panel and gamma-corrected like album
//! art.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::config::Config;
use crate::ha::decode_art;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub struct Frame {
    pictures: Vec<(PathBuf, Vec<u8>)>,
    frames_each: u32,
    alpha: f32,
}

impl Frame {
    pub fn new(cfg: &Config) -> Result<Self> {
        let f = &cfg.frame;
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&f.dir)
            .with_context(|| format!("listing {}", f.dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_picture(p))
            .collect();
        paths.sort();
        if f.shuffle {
            shuffle(&mut paths);
        }
        let mut pictures = Vec::new();
        for p in paths {
            let bytes = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
            match decode_art(&bytes, cfg.gamma) {
                Ok((_, full)) => pictures.push((p, full)),
                Err(e) => eprintln!("panel-ddp: frame: skipping {}: {e:#}", p.display()),
            }
        }
        if pictures.is_empty() {
            bail!("no pictures in {}", f.dir.display());
        }
        eprintln!("panel-ddp: frame: {} pictures from {}", pictures.len(), f.dir.display());
        Ok(Self { pictures, frames_each: ((f.seconds * cfg.fps as f32).round() as u32).max(1), alpha: f.alpha })
    }

    pub fn total_frames(&self) -> u32 {
        self.frames_each * self.pictures.len() as u32
    }

    /// The panel-sized pixels of the picture due at `frame`.
    pub fn picture(&self, frame: u32) -> &[u8] {
        let i = (frame / self.frames_each) as usize % self.pictures.len();
        &self.pictures[i].1
    }

    pub fn draw(&self, c: &mut Canvas, frame: u32) {
        let full = self.picture(frame);
        for j in 0..(WIDTH * HEIGHT) as usize {
            let p = &full[j * 3..j * 3 + 3];
            c.px[j * 3] = (p[0] as f32 * self.alpha) as u8;
            c.px[j * 3 + 1] = (p[1] as f32 * self.alpha) as u8;
            c.px[j * 3 + 2] = (p[2] as f32 * self.alpha) as u8;
        }
    }
}

fn is_picture(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))
}

/// A plain shuffle seeded from the clock; no crate needed for a slideshow.
fn shuffle<T>(v: &mut [T]) {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1;
    for i in (1..v.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        v.swap(i, (seed % (i as u64 + 1)) as usize);
    }
}
