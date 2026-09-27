//! Pages: a loop of dashboard layouts, each shown for its own time, with
//! optional made-up data laid over what the sources report. A demo is just
//! a config whose pages carry all their data; a real dashboard can rotate
//! pages of live tiles the same way. A page without a time stays for good.

use crate::artcache::ArtCache;
use crate::config::PageData;
use crate::model::{Model, Page};
use crate::data::Snapshot;
use crate::ha::{Art, Media, Sensor};
use crate::weather::Weather;
use anyhow::{bail, Result};
use std::sync::Arc;

struct Resolved {
    page: Page,
    start: u32,
    frames: u32,
}

pub struct Pages {
    pages: Vec<Resolved>,
    /// Frames in one pass; None when a page stays for good.
    total: Option<u32>,
    /// Cached covers, newest first, for pages that name one.
    covers: Vec<Art>,
}

/// Where a frame falls: the page, and how far through it, 0..1.
pub struct At<'a> {
    pub page: &'a Page,
    pub t: f32,
}

impl Pages {
    /// `pictures` is how many `[frame]` pictures are loaded. Pages that
    /// name a cover or a picture that is not there are left out.
    pub fn new(model: &Model, pictures: usize) -> Result<Self> {
        let covers = if model.pages.iter().any(|p| p.data.cover.is_some()) {
            let cache = ArtCache::for_model(model);
            // With originals kept, only covers that have one, so an art
            // file only ever gets real covers.
            cache
                .entries(256)
                .into_iter()
                .filter(|e| !model.art_cache.keep_originals || e.original.is_some())
                .enumerate()
                .map(|(i, e)| Art {
                    url: format!("cache:{i}"),
                    scaled: e.scaled,
                    original: e.original.map(Arc::new),
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut pages = Vec::new();
        let mut skipped = 0;
        let mut start = 0u32;
        for p in &model.pages {
            let missing_cover = p.data.cover.is_some_and(|i| i >= covers.len());
            let missing_picture = p.data.picture.is_some_and(|i| i >= pictures);
            if missing_cover || missing_picture {
                skipped += 1;
                continue;
            }
            let frames = p.seconds.map_or(1, |s| ((s * model.fps as f32).round() as u32).max(1));
            pages.push(Resolved { page: p.clone(), start, frames });
            start += frames;
        }
        if skipped > 0 {
            eprintln!(
                "panel-ddp: pages: left out {skipped} naming a cover or picture that is not there ({} covers, {pictures} pictures)",
                covers.len()
            );
        }
        if pages.is_empty() {
            bail!("no page left to show");
        }
        let total = (!model.is_static()).then_some(start);
        Ok(Self { pages, total, covers })
    }

    /// Frames in one pass through the pages; None when a page stays for good.
    pub fn total_frames(&self) -> Option<u32> {
        self.total
    }

    pub fn at(&self, frame: u32) -> At<'_> {
        let Some(total) = self.total else {
            return At { page: &self.pages[0].page, t: 0.0 };
        };
        let f = frame % total;
        let i = self.pages.partition_point(|p| p.start + p.frames <= f);
        let p = &self.pages[i];
        At { page: &p.page, t: (f - p.start) as f32 / p.frames as f32 }
    }

    /// Lays a page's data over the snapshot.
    pub fn apply(&self, data: &PageData, t: f32, snap: &mut Snapshot) {
        if let Some(w) = &data.weather {
            snap.weather = Some(Weather { temperature: w.temperature, code: w.code, is_day: w.is_day });
        }
        for (entity, s) in &data.sensors {
            let state = match (&s.state, s.sweep) {
                (Some(state), _) => state.clone(),
                (None, Some([from, to])) => format!("{}", (from + (to - from) * t as f64).round()),
                (None, None) => continue,
            };
            snap.sensors.insert(entity.clone(), Sensor { state, unit: s.unit.clone() });
        }
        if let Some(i) = data.cover {
            snap.media = Some(Media {
                playing: true,
                title: String::new(),
                artist: String::new(),
                art: Some(self.covers[i].clone()),
            });
        }
    }
}
