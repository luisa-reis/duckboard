//! The `[frame]` pictures: a folder of pictures shown one after another by
//! picture tiles. Each is decoded once, at every size a picture tile shows
//! it at, cropped to that shape and gamma-corrected like album art.

use crate::model::Model;
use crate::picture::{self, Scaled};
use embedded_graphics::prelude::Size;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub struct Frame {
    pictures: Vec<(PathBuf, Scaled)>,
    frames_each: u32,
}

impl Frame {
    /// The pictures in `[frame].dir`, decoded at each of `sizes`.
    pub fn new(cfg: &Model, sizes: &[Size]) -> Result<Self> {
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
            match picture::decode(&bytes, cfg.gamma, sizes) {
                Ok(scaled) => pictures.push((p, scaled)),
                Err(e) => eprintln!("panel-ddp: frame: skipping {}: {e:#}", p.display()),
            }
        }
        if pictures.is_empty() {
            bail!("no pictures in {}", f.dir.display());
        }
        eprintln!("panel-ddp: frame: {} pictures from {}", pictures.len(), f.dir.display());
        Ok(Self { pictures, frames_each: ((f.seconds * cfg.fps as f32).round() as u32).max(1) })
    }

    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// Picture `i`.
    pub fn picture_at(&self, i: usize) -> &Scaled {
        &self.pictures[i % self.pictures.len()].1
    }


    /// The picture due at `frame`.
    pub fn picture(&self, frame: u32) -> &Scaled {
        let i = (frame / self.frames_each) as usize % self.pictures.len();
        &self.pictures[i].1
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
