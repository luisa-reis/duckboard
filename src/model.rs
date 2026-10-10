//! What the panel shows, resolved from a configuration file: the sources,
//! the alerts, and the pages, each a stack of layers in drawing order. The
//! loader looks names up and fills defaults in, so everything that draws or
//! fetches works from this alone.

use crate::config::{
    Alert, ArtCacheConfig, CommandConfig, FrameConfig, HomeAssistantConfig, HttpConfig, PageData, SpotifyConfig, TileSpec, Units, WeatherConfig,
    When,
};
use crate::palette::Palette;
use embedded_graphics::{prelude::*, primitives::Rectangle};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Reads a YAML configuration file. An older TOML or JSON one is refused,
/// with the command that converts it.
pub fn load(path: &std::path::Path) -> anyhow::Result<Model> {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("toml" | "json") => anyhow::bail!(
            "{} is an older TOML or JSON config; convert it with: panel-ddp migrate {}",
            path.display(),
            path.display()
        ),
        _ => crate::format::load(path),
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    /// WLED host, "host" or "host:port".
    pub target: String,
    /// The panel, in pixels.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Applied to pictures before they are sent.
    pub gamma: f32,
    pub temperature: Units,
    /// Gap file for previews; it need not exist.
    pub gaps: PathBuf,
    pub weather: Option<WeatherConfig>,
    pub spotify: Option<SpotifyConfig>,
    pub home_assistant: Option<HomeAssistantConfig>,
    pub http: Option<HttpConfig>,
    pub commands: Vec<CommandConfig>,
    pub art_cache: ArtCacheConfig,
    pub frame: FrameConfig,
    pub alerts: Vec<Alert>,
    /// Where an alert centres its label.
    pub alert_area: Rectangle,
    /// Every page, by the index playlists use; never empty.
    pub pages: Vec<Page>,
    /// Pages shown in turn, looping; each lists at least one page.
    pub playlists: Vec<Playlist>,
    /// Which playlist plays: the first rule that matches the time, checked
    /// at every page's end. When none matches the panel goes dark.
    pub schedule: Vec<Rule>,
    pub art_file: Option<PathBuf>,
    pub art_open: bool,
    /// The files it was read from, watched for changes while running.
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub name: String,
    /// How long the page shows; None keeps it for good, the schedule being
    /// checked every second.
    pub seconds: Option<f32>,
    /// Drawn first to last, each over those before it where it draws.
    pub layers: Vec<Layer>,
    /// Values laid over the live data while the page shows.
    pub data: PageData,
}

#[derive(Debug, Clone)]
pub struct Playlist {
    pub name: String,
    /// Indices into `Model::pages`.
    pub pages: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    /// Index into `Model::playlists`.
    pub playlist: usize,
    /// None matches at any time.
    pub when: Option<When>,
}

/// A tile in an area of the panel, with the colours it draws in.
#[derive(Debug, Clone)]
pub struct Layer {
    pub area: Rectangle,
    pub tile: TileSpec,
    pub palette: Palette,
    /// For a tile on a table's repeat row: the table's data name and which
    /// of the pushed rows it shows. It draws nothing when there is no such
    /// row.
    pub record: Option<(String, usize)>,
}

impl Page {
    /// Whether the album art shows anywhere on the page.
    pub fn shows_art(&self) -> bool {
        self.layers.iter().any(|l| matches!(l.tile, TileSpec::Art { .. }))
    }

    fn wants_pictures(&self) -> bool {
        self.data.picture.is_some() || self.layers.iter().any(|l| matches!(l.tile, TileSpec::Picture { .. }))
    }
}

impl Model {
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Every entity a tile or an alert reads.
    pub fn sensor_entities(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .filter_map(|l| match &l.tile {
                TileSpec::Sensor { entity, .. }
                | TileSpec::Progress { entity, .. }
                | TileSpec::BulletChart { entity: Some(entity), .. } => Some(entity.clone()),
                _ => None,
            })
            .collect();
        v.extend(self.alerts.iter().map(|a| a.entity.clone()));
        v.sort();
        v.dedup();
        v
    }

    /// Every history a chart draws from Home Assistant: the entity and
    /// its hours.
    pub fn series(&self) -> Vec<(String, u32)> {
        let mut v: Vec<(String, u32)> = self
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .filter_map(|l| match l.tile.chart() {
                Some((Some(entity), _, hours)) => Some((entity.clone(), hours)),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// The name of every series a chart draws that is pushed over HTTP.
    pub fn pushed(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .filter_map(|l| l.tile.pushed_series().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// For every table that draws pushed rows, by data name: the columns
    /// its tiles read, each "text", "numbers" (a chart's values) or
    /// "number".
    pub fn table_columns(&self) -> BTreeMap<String, BTreeMap<String, &'static str>> {
        let mut tables: BTreeMap<String, BTreeMap<String, &'static str>> = BTreeMap::new();
        for layer in self.pages.iter().flat_map(|p| &p.layers) {
            let Some((table, _)) = &layer.record else { continue };
            let columns = tables.entry(table.clone()).or_default();
            for (column, holds) in layer.tile.columns() {
                columns.insert(column.clone(), holds);
            }
        }
        tables
    }

    /// The sizes album art is decoded at: every art tile's, smallest first.
    pub fn art_sizes(&self) -> Vec<Size> {
        self.sizes_of(|t| matches!(t, TileSpec::Art { .. }))
    }

    /// The sizes the `[frame]` pictures are decoded at: every picture
    /// tile's, smallest first.
    pub fn picture_sizes(&self) -> Vec<Size> {
        self.sizes_of(|t| matches!(t, TileSpec::Picture { .. }))
    }

    fn sizes_of(&self, kind: impl Fn(&TileSpec) -> bool) -> Vec<Size> {
        let mut v: Vec<Size> =
            self.pages.iter().flat_map(|p| &p.layers).filter(|l| kind(&l.tile)).map(|l| l.area.size).collect();
        v.sort_by_key(|s| (s.width * s.height, s.width, s.height));
        v.dedup();
        v
    }

    /// Whether any page draws the `[frame]` pictures.
    pub fn wants_pictures(&self) -> bool {
        self.pages.iter().any(Page::wants_pictures)
    }

    /// Whether a page stays for good, so there is no pass to play once.
    pub fn is_static(&self) -> bool {
        self.pages.iter().any(|p| p.seconds.is_none())
    }
}
