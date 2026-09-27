//! Pages: dashboard layouts, each shown for its own time, with optional
//! made-up data laid over what the sources report, played from playlists
//! the schedule picks. A demo is just a config whose pages carry all their
//! data; a real dashboard rotates pages of live tiles the same way. A page
//! without a time stays for good.

use crate::artcache::ArtCache;
use crate::config::PageData;
use crate::model::{Model, Page};
use crate::data::Snapshot;
use crate::ha::{Art, Media, Sensor};
use crate::weather::Weather;
use anyhow::{bail, Result};
use chrono::{DateTime, Local};
use std::cell::Cell;
use std::sync::Arc;

/// A page kept for showing, and how many frames it stays: its time, or a
/// second for a page shown for good, after which the schedule is checked
/// and it follows itself again.
struct Kept {
    page: Page,
    frames: u32,
}

/// The page on show: which playlist, where in it, and its frames.
#[derive(Clone, Copy)]
struct Now {
    playlist: usize,
    pos: usize,
    start: u32,
    end: u32,
}

pub struct Pages {
    /// By the model's page index; None for a page left out.
    pages: Vec<Option<Kept>>,
    /// Each playlist's pages that were kept.
    playlists: Vec<Vec<usize>>,
    names: Vec<String>,
    schedule: Vec<crate::model::Rule>,
    /// Whether some page stays for good, so there is no pass to count.
    is_static: bool,
    /// Cached covers, newest first, for pages that name one.
    covers: Vec<Art>,
    now: Cell<Option<Now>>,
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
        let mut skipped = Vec::new();
        let pages: Vec<Option<Kept>> = model
            .pages
            .iter()
            .map(|p| {
                let missing_cover = p.data.cover.is_some_and(|i| i >= covers.len());
                let missing_picture = p.data.picture.is_some_and(|i| i >= pictures);
                if missing_cover || missing_picture {
                    skipped.push(p.name.as_str());
                    return None;
                }
                let frames = p.seconds.map_or(model.fps, |s| ((s * model.fps as f32).round() as u32).max(1));
                Some(Kept { page: p.clone(), frames })
            })
            .collect();
        if !skipped.is_empty() {
            eprintln!(
                "panel-ddp: pages: left out {} naming a cover or picture that is not there ({} covers, {pictures} pictures): {}",
                skipped.len(),
                covers.len(),
                skipped.join(", ")
            );
        }
        let names = model.playlists.iter().map(|l| l.name.clone()).collect();
        let playlists: Vec<Vec<usize>> = model
            .playlists
            .iter()
            .map(|l| l.pages.iter().copied().filter(|&i| pages[i].is_some()).collect())
            .collect();
        if playlists.iter().all(|l| l.is_empty()) {
            bail!("no page left to show");
        }
        Ok(Self {
            pages,
            playlists,
            names,
            schedule: model.schedule.clone(),
            is_static: model.is_static(),
            covers,
            now: Cell::new(None),
        })
    }

    /// The playlist the schedule picks at `now`, if any has pages left.
    fn playlist_at(&self, now: &DateTime<Local>) -> Option<usize> {
        self.schedule
            .iter()
            .find(|r| !self.playlists[r.playlist].is_empty() && r.when.as_ref().is_none_or(|w| w.matches(now)))
            .map(|r| r.playlist)
    }

    fn kept(&self, playlist: usize, pos: usize) -> &Kept {
        self.pages[self.playlists[playlist][pos]].as_ref().expect("playlists hold kept pages")
    }

    /// Frames in one pass through the playlist playing at `now`; None when
    /// a page stays for good or nothing plays.
    pub fn pass_frames(&self, now: &DateTime<Local>) -> Option<u32> {
        if self.is_static {
            return None;
        }
        let l = self.playlist_at(now)?;
        Some((0..self.playlists[l].len()).map(|pos| self.kept(l, pos).frames).sum())
    }

    /// The page for `frame`, frames coming in order. At the end of a page
    /// the schedule picks the playlist: the same one moves on to its next
    /// page, another starts at its first. None while nothing is scheduled.
    pub fn at(&self, frame: u32, now: &DateTime<Local>) -> Option<At<'_>> {
        let current = match self.now.get() {
            Some(n) if frame < n.end => n,
            previous => {
                let playlist = self.playlist_at(now)?;
                let pos = match previous {
                    Some(n) if n.playlist == playlist => (n.pos + 1) % self.playlists[playlist].len(),
                    _ => {
                        if self.playlists.len() > 1 {
                            eprintln!("panel-ddp: playing {}", self.names[playlist]);
                        }
                        0
                    }
                };
                let n = Now { playlist, pos, start: frame, end: frame + self.kept(playlist, pos).frames };
                self.now.set(Some(n));
                n
            }
        };
        let kept = self.kept(current.playlist, current.pos);
        Some(At { page: &kept.page, t: (frame - current.start) as f32 / kept.frames as f32 })
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

#[cfg(test)]
mod tests {
    use super::Pages;
    use crate::config::{TimeOfDay, When};
    use crate::model::{Playlist, Rule};
    use chrono::{DateTime, Local, TimeZone};

    fn model(schedule: Vec<Rule>) -> crate::model::Model {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("panel-ddp-pages-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("p{}.toml", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let page = "[[pages]]\nseconds = 1\n";
        std::fs::write(&path, format!("target = \"x\"\nfps = 10\n{page}{page}{page}")).unwrap();
        let mut m = crate::legacy::load(&path).unwrap();
        m.playlists = vec![
            Playlist { name: "day".into(), pages: vec![0, 1] },
            Playlist { name: "night".into(), pages: vec![2] },
        ];
        m.schedule = schedule;
        m
    }

    fn night() -> Option<When> {
        Some(When { days: vec![], from: TimeOfDay::parse("23:00").ok(), to: TimeOfDay::parse("07:00").ok() })
    }

    fn at(h: u32, m: u32, s: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 25, h, m, s).unwrap()
    }

    /// The page names shown at each frame, a frame being a tenth of a
    /// second from `start`.
    fn shown(p: &Pages, start: DateTime<Local>, frames: u32) -> Vec<String> {
        (0..frames)
            .map(|f| {
                let now = start + chrono::Duration::milliseconds(100 * f as i64);
                p.at(f, &now).map_or("dark".into(), |a| a.page.name.clone())
            })
            .collect()
    }

    #[test]
    fn the_schedule_switches_at_the_end_of_a_page() {
        let m = model(vec![Rule { playlist: 1, when: night() }, Rule { playlist: 0, when: None }]);
        let p = Pages::new(&m, 0).unwrap();
        // 22:59:59.5: page 1 starts before 23:00 and plays its whole second,
        // then night starts with its first page.
        let names = shown(&p, at(22, 59, 59) + chrono::Duration::milliseconds(500), 30);
        assert!(names[..10].iter().all(|n| n == "page 1"));
        assert!(names[10..].iter().all(|n| n == "page 3"));
        assert_eq!(p.pass_frames(&at(12, 0, 0)), Some(20), "day is two pages of a second");
    }

    #[test]
    fn nothing_scheduled_is_dark() {
        let m = model(vec![Rule { playlist: 1, when: night() }]);
        let p = Pages::new(&m, 0).unwrap();
        assert!(shown(&p, at(12, 0, 0), 5).iter().all(|n| n == "dark"));
        assert_eq!(p.pass_frames(&at(12, 0, 0)), None);
        assert!(shown(&p, at(23, 30, 0), 5).iter().all(|n| n == "page 3"));
    }

    #[test]
    fn one_playlist_loops_in_order() {
        let m = model(vec![Rule { playlist: 0, when: None }]);
        let p = Pages::new(&m, 0).unwrap();
        let names = shown(&p, at(12, 0, 0), 40);
        let firsts: Vec<&str> = names.iter().step_by(10).map(String::as_str).collect();
        assert_eq!(firsts, ["page 1", "page 2", "page 1", "page 2"]);
    }
}
