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
    /// Nothing.
    Blank,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HubSpec {
    Blank,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        if cfg.fps == 0 {
            anyhow::bail!("fps must be positive");
        }
        if cfg.gaps.is_relative() {
            if let Some(dir) = path.parent() {
                cfg.gaps = dir.join(&cfg.gaps);
            }
        }
        Ok(cfg)
    }
}
