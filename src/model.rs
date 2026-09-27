//! What the panel shows, resolved from a configuration file: the sources,
//! the alerts, and the pages, each a stack of layers in drawing order. The
//! loader looks names up and fills defaults in, so everything that draws or
//! fetches works from this alone.

use crate::canvas::{HEIGHT, WIDTH};
use crate::config::{
    Alert, ArtCacheConfig, FrameConfig, HomeAssistantConfig, HubSpec, PageData, SpotifyConfig, TileSpec, Units,
    WeatherConfig,
};
use crate::palette::Overrides;
use embedded_graphics::{prelude::*, primitives::Rectangle};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Model {
    /// WLED host, "host" or "host:port".
    pub target: String,
    pub fps: u32,
    /// Applied to pictures before they are sent.
    pub gamma: f32,
    pub temperature: Units,
    /// Gap file for previews; it need not exist.
    pub gaps: PathBuf,
    /// Colour roles for every layer; a layer's own `colors` win.
    pub colors: Overrides,
    pub weather: Option<WeatherConfig>,
    pub spotify: Option<SpotifyConfig>,
    pub home_assistant: Option<HomeAssistantConfig>,
    pub art_cache: ArtCacheConfig,
    pub frame: FrameConfig,
    pub alerts: Vec<Alert>,
    /// Where an alert centres its label.
    pub alert_area: Rectangle,
    /// Shown in turn, never empty.
    pub pages: Vec<Page>,
    pub art_file: Option<PathBuf>,
    pub art_open: bool,
}

#[derive(Debug, Clone)]
pub struct Page {
    /// How long the page shows; None keeps it for good.
    pub seconds: Option<f32>,
    /// Drawn first to last, each over those before it where it draws.
    pub layers: Vec<Layer>,
    /// Values laid over the live data while the page shows.
    pub data: PageData,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub area: Rectangle,
    pub content: Content,
    pub colors: Overrides,
}

#[derive(Debug, Clone)]
pub enum Content {
    Tile(TileSpec),
    Hub(HubSpec),
    /// A picture filling the area, blended over black at `alpha`, in place
    /// of whatever was drawn there.
    Backdrop { source: Backdrop, alpha: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// The album art.
    Media,
    /// The `[frame]` picture due.
    Frame,
}

impl Page {
    /// Whether the album art shows anywhere on the page.
    pub fn shows_art(&self) -> bool {
        self.layers.iter().any(|l| {
            matches!(l.content, Content::Hub(HubSpec::Media { .. }) | Content::Backdrop { source: Backdrop::Media, .. })
        })
    }

    fn wants_pictures(&self) -> bool {
        self.data.picture.is_some()
            || self.layers.iter().any(|l| matches!(l.content, Content::Backdrop { source: Backdrop::Frame, .. }))
    }
}

impl Model {
    /// Every entity a tile or an alert reads.
    pub fn sensor_entities(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .filter_map(|l| match &l.content {
                Content::Tile(TileSpec::Sensor { entity, .. } | TileSpec::Progress { entity, .. }) => Some(entity.clone()),
                _ => None,
            })
            .collect();
        v.extend(self.alerts.iter().map(|a| a.entity.clone()));
        v.sort();
        v.dedup();
        v
    }

    /// The sizes album art is decoded at: the hub's, then the whole panel's
    /// for a background.
    pub fn art_sizes(&self) -> Vec<Size> {
        vec![self.hub_size(), Size::new(WIDTH, HEIGHT)]
    }

    /// The sizes the `[frame]` pictures are decoded at: the whole panel's.
    pub fn picture_sizes(&self) -> Vec<Size> {
        vec![Size::new(WIDTH, HEIGHT)]
    }

    /// The size album art is decoded at for the hub: the first hub's, or
    /// 22x22 when there is none.
    fn hub_size(&self) -> Size {
        self.pages
            .iter()
            .flat_map(|p| &p.layers)
            .find(|l| matches!(l.content, Content::Hub(_)))
            .map_or(Size::new(22, 22), |l| l.area.size)
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
