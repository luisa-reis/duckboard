//! The demo: a crescendo from one tile to the whole dashboard, on made-up
//! data, with every timing from `[demo]` in the config.
//!
//! 1. The date alone, visiting each tile in turn.
//! 2. The clock joins it.
//! 3. The weather, showing every kind of sky.
//! 4. The print progress, filling from 0 to 100.
//! 5. The water leak alert.
//! 6. The whole dashboard over cached album covers, one after another.

use crate::artcache::ArtCache;
use crate::canvas::Canvas;
use crate::config::{Alert, Background, Config, HubEntry, HubSpec, Seconds, TileEntry, TileSpec, Tiles};
use crate::dashboard;
use crate::data::Snapshot;
use crate::ha::{Art, Media, Sensor};
use crate::palette::{Overrides, Palette};
use crate::tiles::Ctx;
use crate::weather::Weather;
use std::collections::HashMap;

/// WMO code and daytime for each sky the weather tile can show.
const SKIES: [(u16, bool); 9] =
    [(0, true), (0, false), (1, true), (1, false), (3, true), (45, true), (61, true), (71, true), (95, true)];
const PROGRESS_ENTITY: &str = "sensor.demo_print_progress";
const LEAK_ENTITY: &str = "binary_sensor.demo_water_leak";
/// The date's tour of the tiles: top left, bottom left, bottom right, and
/// top right, where it stays.
const TOUR: [usize; 4] = [0, 2, 3, 1];

pub struct Demo {
    fps: u32,
    tile: u32,
    clock: u32,
    sky: u32,
    progress: u32,
    alert: u32,
    cover: u32,
    covers: Vec<(Vec<u8>, Vec<u8>)>,
    alert_spec: Alert,
    background_alpha: f32,
    palette: Palette,
    temperature: crate::config::Units,
}

impl Demo {
    pub fn new(cfg: &Config) -> Self {
        let d = &cfg.demo;
        let frames = |secs: f32| ((secs * cfg.fps as f32).round() as u32).max(1);
        let cache = ArtCache::new(cfg.art_cache.dir.clone(), cfg.art_cache.max_bytes(), format!("gamma {}", cfg.gamma));
        let covers = cache.entries(d.covers);
        if covers.is_empty() {
            eprintln!("panel-ddp: demo: no covers in the art cache yet, skipping that step");
        }
        // The alert's look comes from the config's first alert, if any.
        let alert_spec = match cfg.alerts.first() {
            Some(a) => Alert { entity: LEAK_ENTITY.into(), state: "on".into(), ..a.clone() },
            None => Alert {
                entity: LEAK_ENTITY.into(),
                state: "on".into(),
                label: "WATER LEAK".into(),
                color: crate::palette::Rgba::rgb(255, 30, 30),
                pulse_seconds: 1.5,
            },
        };
        Self {
            fps: cfg.fps,
            tile: frames(d.tile_seconds),
            clock: frames(d.clock_seconds),
            sky: frames(d.weather_seconds),
            progress: frames(d.progress_seconds),
            alert: frames(d.alert_seconds),
            cover: frames(d.cover_seconds),
            covers,
            alert_spec,
            background_alpha: d.background_alpha,
            palette: Palette::default().with(&cfg.colors),
            temperature: cfg.temperature,
        }
    }

    /// Frames in one pass.
    pub fn total_frames(&self) -> u32 {
        self.tile * 4
            + self.clock
            + self.sky * SKIES.len() as u32
            + self.progress
            + self.alert
            + self.cover * self.covers.len() as u32
    }

    pub fn draw(&self, c: &mut Canvas, frame: u32) {
        let mut t = frame % self.total_frames();
        // What is on, as the stages accumulate.
        let mut date_tile = 1;
        let mut clock = false;
        let mut sky: Option<(u16, bool)> = None;
        let mut progress: Option<f64> = None;
        let mut leak = false;
        let mut cover: Option<usize> = None;

        let stages: [(u32, u32); 6] = [
            (self.tile, 4),
            (self.clock, 1),
            (self.sky, SKIES.len() as u32),
            (self.progress, 1),
            (self.alert, 1),
            (self.cover, self.covers.len() as u32),
        ];
        let mut stage = 0;
        let mut index = 0;
        let mut within = 0;
        for (i, (len, count)) in stages.iter().enumerate() {
            if t < len * count {
                stage = i;
                index = t / len;
                within = t % len;
                break;
            }
            t -= len * count;
        }
        if stage >= 1 {
            clock = true;
        }
        if stage >= 3 {
            sky = Some(SKIES[0]);
            progress = Some(100.0);
        }
        match stage {
            0 => date_tile = TOUR[index as usize],
            2 => sky = Some(SKIES[index as usize]),
            3 => progress = Some((within as f64 / self.progress as f64 * 100.0).round().min(100.0)),
            4 => leak = true,
            5 => cover = Some(index as usize),
            _ => {}
        }
        if stage == 2 {
            sky = Some(SKIES[index as usize]);
        }

        let tiles = self.tiles(date_tile, clock, sky.is_some(), progress.is_some(), cover.is_some());
        let mut sensors = HashMap::new();
        if let Some(v) = progress {
            sensors.insert(PROGRESS_ENTITY.to_string(), Sensor { state: format!("{v}"), unit: Some("%".into()) });
        }
        sensors.insert(LEAK_ENTITY.to_string(), Sensor { state: if leak { "on" } else { "off" }.into(), unit: None });
        let media = cover.map(|i| {
            let (rgb, full) = self.covers[i].clone();
            Media { playing: true, title: String::new(), artist: String::new(), art: Some(Art { url: String::new(), rgb, full }) }
        });
        let data = Snapshot {
            weather: sky.map(|(code, is_day)| Weather { temperature: 72.0, code, is_day }),
            sensors,
            media,
        };
        let ctx = Ctx {
            now: chrono::Local::now(),
            frame,
            data: &data,
            palette: &self.palette,
            temperature: self.temperature,
            fps: self.fps,
        };
        dashboard::draw(&tiles, std::slice::from_ref(&self.alert_spec), c, &ctx);
    }

    fn tiles(&self, date_tile: usize, clock: bool, weather: bool, progress: bool, background: bool) -> Tiles {
        let entry = |spec: TileSpec| TileEntry { spec, colors: Overrides::default() };
        let mut tiles = [
            entry(TileSpec::Blank),
            entry(TileSpec::Blank),
            entry(TileSpec::Blank),
            entry(TileSpec::Blank),
        ];
        tiles[date_tile] = entry(TileSpec::Date);
        if clock {
            tiles[0] = entry(TileSpec::Clock { seconds: Seconds::Dot, dot_size: 2 });
        }
        if weather {
            tiles[2] = entry(TileSpec::Weather);
        }
        if progress {
            tiles[3] = entry(TileSpec::Progress {
                entity: PROGRESS_ENTITY.into(),
                label: "PRINT".into(),
                max: 100.0,
                decimals: 0,
            });
        }
        let [top_left, top_right, bottom_left, bottom_right] = tiles;
        Tiles {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            hub: HubEntry { spec: HubSpec::Blank, colors: Overrides::default() },
            background: Some(if background {
                Background::Media { alpha: self.background_alpha }
            } else {
                Background::None
            }),
        }
    }
}
