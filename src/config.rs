//! The dashboard configuration, a TOML file. See dashboard.example.toml.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// WLED host, "host" or "host:port".
    pub target: String,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Gap file for previews, relative to the config file. Optional: without
    /// it the preview shows the whole panel.
    #[serde(default = "default_gaps")]
    pub gaps: PathBuf,
    #[serde(default)]
    pub tiles: Tiles,
    pub weather: Option<WeatherConfig>,
    pub spotify: Option<SpotifyConfig>,
    pub home_assistant: Option<HomeAssistantConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpotifyConfig {
    /// The Client ID of an app from developer.spotify.com/dashboard.
    pub client_id: String,
    /// Where `panel-ddp spotify-login` keeps the tokens, relative to the
    /// config file.
    #[serde(default = "default_token_file")]
    pub token_file: PathBuf,
    #[serde(default = "default_spotify_refresh")]
    pub refresh_seconds: u64,
}

fn default_token_file() -> PathBuf {
    "spotify-token.json".into()
}

fn default_spotify_refresh() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistantConfig {
    /// e.g. "http://homeassistant.local:8123"
    pub url: String,
    /// A long-lived access token (profile page, bottom).
    pub token: String,
    /// The media_player entity for the now_playing tile and media hub.
    pub media_player: Option<String>,
    #[serde(default = "default_refresh_seconds")]
    pub refresh_seconds: u64,
}

fn default_refresh_seconds() -> u64 {
    10
}

fn default_max() -> f64 {
    100.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub units: Units,
    #[serde(default = "default_refresh_minutes")]
    pub refresh_minutes: u64,
}

fn default_refresh_minutes() -> u64 {
    10
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}

impl Units {
    pub fn api_name(self) -> &'static str {
        match self {
            Units::Celsius => "celsius",
            Units::Fahrenheit => "fahrenheit",
        }
    }
}

fn default_fps() -> u32 {
    10
}

fn default_gaps() -> PathBuf {
    "2d-gaps.json".into()
}

/// What each tile shows, and the hub in the middle.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tiles {
    #[serde(default = "Tiles::default_top_left")]
    pub top_left: TileSpec,
    #[serde(default = "Tiles::default_top_right")]
    pub top_right: TileSpec,
    #[serde(default = "Tiles::default_bottom_left")]
    pub bottom_left: TileSpec,
    #[serde(default = "Tiles::default_bottom_right")]
    pub bottom_right: TileSpec,
    #[serde(default = "Tiles::default_hub")]
    pub hub: HubSpec,
}

impl Tiles {
    fn default_top_left() -> TileSpec {
        TileSpec::Clock
    }
    fn default_top_right() -> TileSpec {
        TileSpec::Date
    }
    fn default_bottom_left() -> TileSpec {
        TileSpec::Blank
    }
    fn default_bottom_right() -> TileSpec {
        TileSpec::Blank
    }
    fn default_hub() -> HubSpec {
        HubSpec::Blank
    }
}

impl Default for Tiles {
    fn default() -> Self {
        Self {
            top_left: Self::default_top_left(),
            top_right: Self::default_top_right(),
            bottom_left: Self::default_bottom_left(),
            bottom_right: Self::default_bottom_right(),
            hub: Self::default_hub(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TileSpec {
    /// Hours over minutes, with a seconds bar.
    Clock,
    /// Weekday, day of month, month.
    Date,
    /// Sky icon and temperature; needs the [weather] table.
    Weather,
    /// A Home Assistant entity's state under a label; needs [home_assistant].
    Sensor {
        entity: String,
        label: String,
        /// Overrides the entity's unit_of_measurement; "" hides it.
        unit: Option<String>,
        /// Decimal places for a numeric state; fewer are used if it does not fit.
        decimals: Option<u8>,
    },
    /// A Home Assistant entity's numeric state as a bar under the value,
    /// full at `max`; needs [home_assistant].
    Progress {
        entity: String,
        label: String,
        #[serde(default = "default_max")]
        max: f64,
        /// Decimal places for the value; 0 unless set.
        #[serde(default)]
        decimals: u8,
    },
    /// Artist and title scrolling; needs [home_assistant].media_player.
    NowPlaying,
    /// Nothing.
    Blank,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HubSpec {
    Blank,
    /// Album art as a spinning disc; needs [home_assistant].media_player.
    Media,
}

impl Config {
    /// The four corner tiles, in reading order.
    pub fn corner_tiles(&self) -> [&TileSpec; 4] {
        [&self.tiles.top_left, &self.tiles.top_right, &self.tiles.bottom_left, &self.tiles.bottom_right]
    }

    /// Whether something can feed the now_playing tile and the media hub.
    /// Spotify takes precedence over a Home Assistant media player.
    pub fn has_media_source(&self) -> bool {
        self.spotify.is_some() || self.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some())
    }

    /// Every entity a sensor tile reads.
    pub fn sensor_entities(&self) -> Vec<String> {
        self.corner_tiles()
            .iter()
            .filter_map(|t| match t {
                TileSpec::Sensor { entity, .. } | TileSpec::Progress { entity, .. } => Some(entity.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        if cfg.fps == 0 {
            anyhow::bail!("fps must be positive");
        }
        for t in cfg.corner_tiles() {
            if let TileSpec::Progress { max, label, .. } = t {
                if max.is_nan() || *max <= 0.0 {
                    anyhow::bail!("progress tile {label}: max must be positive");
                }
            }
        }
        let tiles = cfg.corner_tiles();
        if cfg.weather.is_none() && tiles.iter().any(|t| matches!(t, TileSpec::Weather)) {
            anyhow::bail!("a weather tile needs the [weather] table");
        }
        let needs_ha = tiles.iter().any(|t| matches!(t, TileSpec::Sensor { .. } | TileSpec::Progress { .. }));
        if needs_ha && cfg.home_assistant.is_none() {
            anyhow::bail!("sensor and progress tiles need the [home_assistant] table");
        }
        let needs_player = tiles.iter().any(|t| matches!(t, TileSpec::NowPlaying))
            || matches!(cfg.tiles.hub, HubSpec::Media);
        if needs_player && !cfg.has_media_source() {
            anyhow::bail!("now_playing and media tiles need a [spotify] table or [home_assistant].media_player");
        }
        if let Some(dir) = path.parent() {
            if cfg.gaps.is_relative() {
                cfg.gaps = dir.join(&cfg.gaps);
            }
            if let Some(sp) = cfg.spotify.as_mut() {
                if sp.token_file.is_relative() {
                    sp.token_file = dir.join(&sp.token_file);
                }
            }
        }
        Ok(cfg)
    }
}
