//! The demo: a crescendo from one tile to the whole dashboard, on made-up
//! data, with every timing from `[demo]` in the config.
//!
//! 1. The date alone, visiting each tile in turn.
//! 2. The clock joins it.
//! 3. The weather, showing every kind of sky.
//! 4. The print progress, filling from 0 to 100.
//! 5. The laundry temperature in its place.
//! 6. The water leak alert.
//! 7. The whole dashboard again, plain.
//! 8. The same with a cover as a disc in the hub.
//! 9. The same over cached album covers, one after another.
//! 10. The covers alone, nothing else drawn.
//! 11. A few of the `[frame]` pictures, full screen.
//! 12. The dashboard with those pictures behind it.

use crate::artcache::{ArtCache, Entry};
use crate::canvas::Canvas;
use crate::config::{Alert, ArtShape, Background, Config, HubEntry, HubSpec, Seconds, TileEntry, TileSpec, Tiles};
use crate::dashboard;
use crate::data::Snapshot;
use crate::frame::Frame;
use crate::ha::{Art, Media, Sensor};
use crate::palette::{Overrides, Palette};
use crate::tiles::Ctx;
use crate::canvas::{HEIGHT, WIDTH};
use crate::weather::Weather;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

/// Scale of the companion image over the panel.
const ART_FILE_SCALE: u32 = 4;

/// WMO code and daytime for each sky the weather tile can show.
const SKIES: [(u16, bool); 9] =
    [(0, true), (0, false), (1, true), (1, false), (3, true), (45, true), (61, true), (71, true), (95, true)];
const PROGRESS_ENTITY: &str = "sensor.demo_print_progress";
const SENSOR_ENTITY: &str = "sensor.demo_laundry_temperature";
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
    sensor: u32,
    alert: u32,
    dashboard: u32,
    hub: u32,
    cover: u32,
    art_only: u32,
    frame_each: u32,
    covers: Vec<Entry>,
    pictures: Option<Frame>,
    n_pictures: u32,
    alert_spec: Alert,
    background_alpha: f32,
    palette: Palette,
    temperature: crate::config::Units,
    art_file: Option<PathBuf>,
    art_open: bool,
    /// Which cover the companion image holds; None is black.
    written: Cell<Option<Option<usize>>>,
}

impl Demo {
    pub fn new(cfg: &Config) -> Self {
        let d = &cfg.demo;
        let frames = |secs: f32| ((secs * cfg.fps as f32).round() as u32).max(1);
        let cache = ArtCache::new(
            cfg.art_cache.dir.clone(),
            cfg.art_cache.max_bytes(),
            format!("gamma {}", cfg.gamma),
            cfg.art_cache.keep_originals,
        );
        // Only covers with the picture as downloaded, so the art file never
        // gets a scaled-up panel version.
        let covers: Vec<Entry> = cache.entries(d.covers * 4).into_iter().filter(|e| e.original.is_some()).take(d.covers).collect();
        if covers.is_empty() {
            eprintln!("panel-ddp: demo: no covers with originals in the art cache yet (keep_originals), skipping those steps");
        }
        let pictures = match Frame::new(cfg) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("panel-ddp: demo: no frame pictures ({e:#}), skipping those steps");
                None
            }
        };
        let n_pictures = pictures.as_ref().map_or(0, |f| f.len().min(d.frame_pictures)) as u32;
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
            sensor: frames(d.sensor_seconds),
            alert: frames(d.alert_seconds),
            dashboard: frames(d.dashboard_seconds),
            hub: frames(d.hub_seconds),
            cover: frames(d.cover_seconds),
            art_only: frames(d.art_only_seconds),
            frame_each: frames(d.frame_seconds),
            covers,
            pictures,
            n_pictures,
            alert_spec,
            background_alpha: d.background_alpha,
            palette: Palette::default().with(&cfg.colors),
            temperature: cfg.temperature,
            art_file: d.art_file.clone(),
            art_open: d.art_open,
            written: Cell::new(None),
        }
    }

    /// Rewrites the companion image when the cover changes.
    fn write_art_file(&self, cover: Option<usize>) {
        let Some(path) = &self.art_file else { return };
        if self.written.get() == Some(cover) {
            return;
        }
        // The original picture, re-encoded as JPEG whatever it came as;
        // black when there is no cover on.
        let img = match cover.and_then(|i| self.covers[i].original.as_deref()).and_then(|b| image::load_from_memory(b).ok()) {
            Some(orig) => orig.to_rgb8(),
            None => image::RgbImage::new(WIDTH * ART_FILE_SCALE, HEIGHT * ART_FILE_SCALE),
        };
        // Written whole and renamed into place, so a viewer never reads a
        // half-written file and sees one change per cover.
        let tmp = path.with_extension("jpg.tmp");
        let result = img
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())
            .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.written.set(Some(cover));
                if self.art_open {
                    // Preview re-reads a file it is told to open.
                    let _ = std::process::Command::new("open").arg(path).spawn();
                }
            }
            Err(e) => eprintln!("panel-ddp: demo: writing {}: {e}", path.display()),
        }
    }

    /// Frames in one pass.
    pub fn total_frames(&self) -> u32 {
        self.tile * 4
            + self.clock
            + self.sky * SKIES.len() as u32
            + self.progress
            + self.sensor
            + self.alert
            + self.dashboard
            + self.hub * self.covers.len().min(1) as u32
            + self.cover * self.covers.len() as u32
            + self.art_only * self.covers.len() as u32
            + self.frame_each * self.n_pictures * 2
    }

    pub fn draw(&self, c: &mut Canvas, frame: u32) {
        let mut t = frame % self.total_frames();
        // What is on, as the stages accumulate.
        let mut date_tile = 1;
        let mut clock = false;
        let mut sky: Option<(u16, bool)> = None;
        let mut progress: Option<f64> = None;
        let mut sensor = false;
        let mut leak = false;
        let mut cover: Option<usize> = None;
        let mut placement = Placement::Background;
        let mut picture: Option<usize> = None;

        let n_covers = self.covers.len() as u32;
        let stages: [(u32, u32); 12] = [
            (self.tile, 4),
            (self.clock, 1),
            (self.sky, SKIES.len() as u32),
            (self.progress, 1),
            (self.sensor, 1),
            (self.alert, 1),
            (self.dashboard, 1),
            (self.hub, n_covers.min(1)),
            (self.cover, n_covers),
            (self.art_only, n_covers),
            (self.frame_each, self.n_pictures),
            (self.frame_each, self.n_pictures),
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
        if stage >= 4 {
            // The print is done; the tile shows the laundry from here on.
            progress = None;
            sensor = true;
        }
        match stage {
            0 => date_tile = TOUR[index as usize],
            2 => sky = Some(SKIES[index as usize]),
            3 => progress = Some((within as f64 / self.progress as f64 * 100.0).round().min(100.0)),
            5 => leak = true,
            7 => {
                cover = Some(0);
                placement = Placement::Hub;
            }
            8 => cover = Some(index as usize),
            9 => {
                cover = Some(index as usize);
                placement = Placement::Alone;
            }
            10 => {
                picture = Some(index as usize);
                placement = Placement::Alone;
            }
            11 => picture = Some(index as usize),
            _ => {}
        }
        if stage == 2 {
            sky = Some(SKIES[index as usize]);
        }

        self.write_art_file(cover);
        let tiles = self.tiles(
            date_tile,
            clock,
            sky.is_some(),
            progress.is_some(),
            sensor,
            cover.is_some(),
            picture.is_some(),
            placement,
        );
        let mut sensors = HashMap::new();
        if let Some(v) = progress {
            sensors.insert(PROGRESS_ENTITY.to_string(), Sensor { state: format!("{v}"), unit: Some("%".into()) });
        }
        // 29 °C, shown in whichever unit the config asks for.
        sensors.insert(SENSOR_ENTITY.to_string(), Sensor { state: "29".into(), unit: Some("°C".into()) });
        sensors.insert(LEAK_ENTITY.to_string(), Sensor { state: if leak { "on" } else { "off" }.into(), unit: None });
        let media = cover.map(|i| {
            let e = &self.covers[i];
            Media {
                playing: true,
                title: String::new(),
                artist: String::new(),
                art: Some(Art { url: String::new(), rgb: e.hub.clone(), full: e.full.clone() }),
            }
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
            picture: picture.and_then(|i| self.pictures.as_ref().map(|f| f.picture_at(i))),
        };
        dashboard::draw(&tiles, std::slice::from_ref(&self.alert_spec), c, &ctx);
    }

    #[allow(clippy::too_many_arguments)]
    fn tiles(
        &self,
        date_tile: usize,
        clock: bool,
        weather: bool,
        progress: bool,
        sensor: bool,
        art: bool,
        picture: bool,
        placement: Placement,
    ) -> Tiles {
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
            tiles[3] = entry(TileSpec::Weather);
        }
        if progress {
            tiles[2] = entry(TileSpec::Progress {
                entity: PROGRESS_ENTITY.into(),
                label: "PRINT".into(),
                max: 100.0,
                decimals: 0,
            });
        }
        if sensor {
            tiles[2] = entry(TileSpec::Sensor {
                entity: SENSOR_ENTITY.into(),
                label: "LNDRY".into(),
                unit: None,
                decimals: None,
            });
        }
        if (art || picture) && placement == Placement::Alone {
            tiles = [entry(TileSpec::Blank), entry(TileSpec::Blank), entry(TileSpec::Blank), entry(TileSpec::Blank)];
        }
        let [top_left, top_right, bottom_left, bottom_right] = tiles;
        let hub = if art && placement == Placement::Hub {
            HubSpec::Media { spin: false, shape: ArtShape::Disc, paused_alpha: 0.4, corner_alpha: 0.3 }
        } else {
            HubSpec::Blank
        };
        let background = match (art, picture, placement) {
            (_, true, Placement::Alone) => Background::Frame { alpha: 1.0 },
            (_, true, _) => Background::Frame { alpha: self.background_alpha },
            (false, _, _) | (true, _, Placement::Hub) => Background::None,
            (true, _, Placement::Background) => Background::Media { alpha: self.background_alpha },
            (true, _, Placement::Alone) => Background::Media { alpha: 1.0 },
        };
        Tiles {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            hub: HubEntry { spec: hub, colors: Overrides::default() },
            background: Some(background),
        }
    }
}

/// Where a cover goes in a stage.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    Hub,
    Background,
    Alone,
}

impl Demo {
}
