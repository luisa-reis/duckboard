//! The dashboard configuration, a TOML file. See dashboard.example.toml.

use crate::palette::{Overrides, Rgba};
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
    /// Gamma applied to pictures before they are sent. WLED gamma-corrects
    /// its own effects but not streamed frames, and the HUB75 driver is
    /// linear, so sRGB pictures need 2.2 to look as they should. Set 1.0
    /// if the board's realtime gamma correction is switched on.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// The unit temperatures are shown in: the weather, and any sensor whose
    /// reading is in degrees, converted if Home Assistant reports the other.
    #[serde(default)]
    pub temperature: Units,
    /// Gap file for previews, relative to the config file. Optional: without
    /// it the preview shows the whole panel.
    #[serde(default = "default_gaps")]
    pub gaps: PathBuf,
    #[serde(default)]
    pub tiles: Tiles,
    /// Colour roles for every tile; a tile's own `colors` wins over these.
    #[serde(default)]
    pub colors: Overrides,
    pub weather: Option<WeatherConfig>,
    pub spotify: Option<SpotifyConfig>,
    pub home_assistant: Option<HomeAssistantConfig>,
    #[serde(default)]
    pub art_cache: ArtCacheConfig,
    /// While any of these entities is in its state, the panel drops the
    /// tiles, pulses in the alert's colour and shows its label.
    #[serde(default)]
    pub alerts: Vec<Alert>,
    /// Timings for `panel-ddp demo`.
    #[serde(default)]
    pub demo: DemoConfig,
}

/// How long each step of the demo lasts, in seconds.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemoConfig {
    /// The date alone, in each tile in turn.
    #[serde(default = "d_tile")]
    pub tile_seconds: f32,
    /// After the clock joins it.
    #[serde(default = "d_clock")]
    pub clock_seconds: f32,
    /// Each kind of sky on the weather tile.
    #[serde(default = "d_weather")]
    pub weather_seconds: f32,
    /// The print progress filling from 0 to 100.
    #[serde(default = "d_progress")]
    pub progress_seconds: f32,
    /// The water leak alert.
    #[serde(default = "d_alert")]
    pub alert_seconds: f32,
    /// The whole dashboard after the alert, before any cover.
    #[serde(default = "d_dashboard")]
    pub dashboard_seconds: f32,
    /// The dashboard with a cover as a disc in the hub.
    #[serde(default = "d_hub")]
    pub hub_seconds: f32,
    /// Each cover as the background, from the art cache.
    #[serde(default = "d_cover")]
    pub cover_seconds: f32,
    /// Each cover on its own, nothing else drawn, at the end.
    #[serde(default = "d_art_only")]
    pub art_only_seconds: f32,
    /// How many cached covers to show, newest first.
    #[serde(default = "d_covers")]
    pub covers: usize,
    /// Alpha of the cover behind the tiles.
    #[serde(default = "d_alpha")]
    pub background_alpha: f32,
    /// A JPEG rewritten with the current cover as the demo proceeds, black
    /// when there is none; relative to the config file. Open it in Preview
    /// for a companion view.
    pub art_file: Option<PathBuf>,
    /// Run `open` on the art file after each change, so macOS Preview shows
    /// and re-reads it.
    #[serde(default)]
    pub art_open: bool,
}

fn d_tile() -> f32 {
    3.0
}
fn d_clock() -> f32 {
    5.0
}
fn d_weather() -> f32 {
    2.5
}
fn d_progress() -> f32 {
    10.0
}
fn d_alert() -> f32 {
    5.0
}
fn d_dashboard() -> f32 {
    5.0
}
fn d_hub() -> f32 {
    5.0
}
fn d_art_only() -> f32 {
    4.0
}
fn d_cover() -> f32 {
    4.0
}
fn d_covers() -> usize {
    8
}
fn d_alpha() -> f32 {
    0.12
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            tile_seconds: d_tile(),
            clock_seconds: d_clock(),
            weather_seconds: d_weather(),
            progress_seconds: d_progress(),
            alert_seconds: d_alert(),
            dashboard_seconds: d_dashboard(),
            hub_seconds: d_hub(),
            cover_seconds: d_cover(),
            art_only_seconds: d_art_only(),
            covers: d_covers(),
            background_alpha: d_alpha(),
            art_file: None,
            art_open: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    pub entity: String,
    /// The state that raises the alert.
    #[serde(default = "default_alert_state")]
    pub state: String,
    /// Shown in the middle of the panel: up to four characters large in
    /// the hub, up to eleven small on the band above it.
    pub label: String,
    #[serde(default = "default_alert_color")]
    pub color: Rgba,
    /// One pulse, in seconds.
    #[serde(default = "default_pulse_seconds")]
    pub pulse_seconds: f32,
}

fn default_alert_state() -> String {
    "on".into()
}

fn default_alert_color() -> Rgba {
    Rgba::rgb(255, 30, 30)
}

fn default_pulse_seconds() -> f32 {
    1.5
}

/// Where decoded album art is kept between runs, and how much of it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtCacheConfig {
    /// Relative to the config file.
    #[serde(default = "default_cache_dir")]
    pub dir: PathBuf,
    /// 0 disables the cache.
    #[serde(default = "default_cache_megabytes")]
    pub max_megabytes: f64,
    /// Also keep each picture as downloaded, at its original size, and ask
    /// Spotify for its largest. The demo's `art_file` is then written from
    /// it. Costs a few tens of kilobytes per cover against the cap.
    #[serde(default)]
    pub keep_originals: bool,
}

fn default_cache_dir() -> PathBuf {
    "art-cache".into()
}

fn default_cache_megabytes() -> f64 {
    4.0
}

impl Default for ArtCacheConfig {
    fn default() -> Self {
        Self { dir: default_cache_dir(), max_megabytes: default_cache_megabytes(), keep_originals: false }
    }
}

impl ArtCacheConfig {
    pub fn max_bytes(&self) -> u64 {
        (self.max_megabytes.max(0.0) * 1024.0 * 1024.0) as u64
    }
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
    /// Overrides the top-level `temperature` for the weather tile alone.
    /// Left out, `Config::load` fills it in.
    pub units: Option<Units>,
    #[serde(default = "default_refresh_minutes")]
    pub refresh_minutes: u64,
}

fn default_refresh_minutes() -> u64 {
    10
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Celsius,
    #[default]
    Fahrenheit,
}

impl Units {
    pub fn api_name(self) -> &'static str {
        match self {
            Units::Celsius => "celsius",
            Units::Fahrenheit => "fahrenheit",
        }
    }

    /// A reading in degrees, converted to this unit when its symbol says
    /// it is in the other; anything else comes back unchanged.
    pub fn convert(self, value: f64, unit: &str) -> (f64, String) {
        match (unit, self) {
            ("°C", Units::Fahrenheit) => (value * 9.0 / 5.0 + 32.0, "°F".into()),
            ("°F", Units::Celsius) => ((value - 32.0) * 5.0 / 9.0, "°C".into()),
            _ => (value, unit.into()),
        }
    }
}

fn default_fps() -> u32 {
    10
}

fn default_gamma() -> f32 {
    2.2
}

fn default_gaps() -> PathBuf {
    "2d-gaps.json".into()
}

/// What each tile shows, and the hub in the middle.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tiles {
    #[serde(default = "Tiles::default_top_left")]
    pub top_left: TileEntry,
    #[serde(default = "Tiles::default_top_right")]
    pub top_right: TileEntry,
    #[serde(default = "Tiles::default_bottom_left")]
    pub bottom_left: TileEntry,
    #[serde(default = "Tiles::default_bottom_right")]
    pub bottom_right: TileEntry,
    #[serde(default = "Tiles::default_hub")]
    pub hub: HubEntry,
    /// Painted over the whole panel before the tiles. The art shows in one
    /// place: here unless the hub is `media`, or switched off. Left unset in
    /// the file, `Config::load` resolves it.
    #[serde(default)]
    pub background: Option<Background>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Background {
    None,
    /// The album art across the panel, blended over black at `alpha` so
    /// the tiles stay legible.
    Media {
        /// 0..1, the art's opacity.
        #[serde(default = "default_background_alpha")]
        alpha: f32,
    },
}

fn default_background_alpha() -> f32 {
    0.12
}

/// A tile with its own colour overrides. Deserialised by hand:
/// `colors` is lifted out and the rest goes to the tile kind, which then
/// still rejects keys it does not know (serde's `flatten` would not).
#[derive(Debug, Clone)]
pub struct TileEntry {
    pub spec: TileSpec,
    pub colors: Overrides,
}

/// Splits a tile table into its `colors` and the rest. A kind without
/// fields of its own is checked here, because serde lets a unit variant
/// ignore stray keys.
fn split_colors<'de, D: serde::Deserializer<'de>>(
    d: D,
    unit_kinds: &[&str],
) -> Result<(toml::Table, Overrides), D::Error> {
    let mut table = toml::Table::deserialize(d)?;
    let colors = match table.remove("colors") {
        Some(v) => v.try_into().map_err(serde::de::Error::custom)?,
        None => Overrides::default(),
    };
    if let Some(kind) = table.get("kind").and_then(|k| k.as_str()) {
        if unit_kinds.contains(&kind) {
            if let Some(stray) = table.keys().find(|k| *k != "kind") {
                return Err(serde::de::Error::custom(format!("unknown field `{stray}` for kind {kind}")));
            }
        }
    }
    Ok((table, colors))
}

impl<'de> Deserialize<'de> for TileEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (rest, colors) = split_colors(d, &["date", "weather", "now_playing", "blank"])?;
        let spec = rest.try_into().map_err(serde::de::Error::custom)?;
        Ok(Self { spec, colors })
    }
}

impl From<TileSpec> for TileEntry {
    fn from(spec: TileSpec) -> Self {
        Self { spec, colors: Overrides::default() }
    }
}

#[derive(Debug, Clone)]
pub struct HubEntry {
    pub spec: HubSpec,
    pub colors: Overrides,
}

impl<'de> Deserialize<'de> for HubEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (rest, colors) = split_colors(d, &["blank"])?;
        let spec = rest.try_into().map_err(serde::de::Error::custom)?;
        Ok(Self { spec, colors })
    }
}

impl Tiles {
    fn default_top_left() -> TileEntry {
        TileSpec::Clock { seconds: Seconds::Dot, dot_size: default_dot_size() }.into()
    }
    fn default_top_right() -> TileEntry {
        TileSpec::Date.into()
    }
    fn default_bottom_left() -> TileEntry {
        TileSpec::Blank.into()
    }
    fn default_bottom_right() -> TileEntry {
        TileSpec::Blank.into()
    }
    fn default_hub() -> HubEntry {
        HubEntry { spec: HubSpec::Blank, colors: Overrides::default() }
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
            background: None,
        }
    }
}

fn default_dot_size() -> u32 {
    2
}

/// How the clock shows the seconds on its ring.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Seconds {
    /// The ring fills clockwise from twelve.
    Ring,
    /// A single dot travels round the ring, like a second hand.
    #[default]
    Dot,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TileSpec {
    /// Hours over minutes inside a seconds ring.
    Clock {
        #[serde(default)]
        seconds: Seconds,
        /// Ring pixels the seconds dot covers.
        #[serde(default = "default_dot_size")]
        dot_size: u32,
    },
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
    /// Album art; needs [spotify] or [home_assistant].media_player.
    Media {
        /// Turn the art while playing.
        #[serde(default)]
        spin: bool,
        #[serde(default)]
        shape: ArtShape,
        /// The art's alpha while paused.
        #[serde(default = "default_paused_alpha")]
        paused_alpha: f32,
        /// The alpha of the corners outside the circle in the faded shape.
        #[serde(default = "default_corner_alpha")]
        corner_alpha: f32,
    },
}

fn default_paused_alpha() -> f32 {
    0.4
}

fn default_corner_alpha() -> f32 {
    0.3
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtShape {
    /// A record: circular, with a spindle hole.
    #[default]
    Disc,
    /// The whole cover.
    Square,
    /// The whole cover, the corners outside the circle dimmed.
    Faded,
}

impl Config {
    /// The four corner tiles, in reading order.
    pub fn corner_tiles(&self) -> [&TileSpec; 4] {
        [&self.tiles.top_left.spec, &self.tiles.top_right.spec, &self.tiles.bottom_left.spec, &self.tiles.bottom_right.spec]
    }

    /// Whether something can feed the now_playing tile and the media hub.
    /// Spotify takes precedence over a Home Assistant media player.
    pub fn has_media_source(&self) -> bool {
        self.spotify.is_some() || self.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some())
    }

    /// Every entity a sensor tile or an alert reads.
    pub fn sensor_entities(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .corner_tiles()
            .iter()
            .filter_map(|t| match t {
                TileSpec::Sensor { entity, .. } | TileSpec::Progress { entity, .. } => Some(entity.clone()),
                _ => None,
            })
            .collect();
        v.extend(self.alerts.iter().map(|a| a.entity.clone()));
        v.dedup();
        v
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        if cfg.fps == 0 {
            anyhow::bail!("fps must be positive");
        }
        if !(0.5..=5.0).contains(&cfg.gamma) {
            anyhow::bail!("gamma must be between 0.5 and 5");
        }
        if let Some(w) = cfg.weather.as_mut() {
            w.units.get_or_insert(cfg.temperature);
        }
        if cfg.art_cache.max_megabytes.is_nan() || cfg.art_cache.max_megabytes < 0.0 {
            anyhow::bail!("art_cache.max_megabytes must be 0 or more");
        }
        for t in cfg.corner_tiles() {
            match t {
                TileSpec::Progress { max, label, .. } if max.is_nan() || *max <= 0.0 => {
                    anyhow::bail!("progress tile {label}: max must be positive")
                }
                TileSpec::Clock { dot_size, .. } if !(1..=12).contains(dot_size) => {
                    anyhow::bail!("clock dot_size must be between 1 and 12")
                }
                _ => {}
            }
        }
        let tiles = cfg.corner_tiles();
        if cfg.weather.is_none() && tiles.iter().any(|t| matches!(t, TileSpec::Weather)) {
            anyhow::bail!("a weather tile needs the [weather] table");
        }
        let needs_ha = tiles.iter().any(|t| matches!(t, TileSpec::Sensor { .. } | TileSpec::Progress { .. }))
            || !cfg.alerts.is_empty();
        if needs_ha && cfg.home_assistant.is_none() {
            anyhow::bail!("sensor and progress tiles, and alerts, need the [home_assistant] table");
        }
        {
            let d = &cfg.demo;
            let times = [
                d.tile_seconds,
                d.clock_seconds,
                d.weather_seconds,
                d.progress_seconds,
                d.alert_seconds,
                d.dashboard_seconds,
                d.hub_seconds,
                d.cover_seconds,
                d.art_only_seconds,
            ];
            if times.iter().any(|t| t.is_nan() || *t <= 0.0) {
                anyhow::bail!("[demo] timings must be positive seconds");
            }
            if !(0.0..=1.0).contains(&d.background_alpha) {
                anyhow::bail!("[demo] background_alpha must be between 0 and 1");
            }
        }
        for a in &cfg.alerts {
            if a.label.chars().count() > 11 || a.label.is_empty() {
                anyhow::bail!("alert {}: the label is one to eleven characters", a.entity);
            }
            if a.pulse_seconds.is_nan() || a.pulse_seconds <= 0.0 {
                anyhow::bail!("alert {}: pulse_seconds must be positive", a.entity);
            }
        }
        let needs_player = tiles.iter().any(|t| matches!(t, TileSpec::NowPlaying))
            || matches!(cfg.tiles.hub.spec, HubSpec::Media { .. });
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
            if cfg.art_cache.dir.is_relative() {
                cfg.art_cache.dir = dir.join(&cfg.art_cache.dir);
            }
            if let Some(f) = cfg.demo.art_file.as_mut() {
                if f.is_relative() {
                    *f = dir.join(&*f);
                }
            }
        }
        // Last, after the tiles borrow ends: where the art goes.
        let hub_has_art = matches!(cfg.tiles.hub.spec, HubSpec::Media { .. });
        if let HubSpec::Media { paused_alpha, corner_alpha, .. } = cfg.tiles.hub.spec {
            if !(0.0..=1.0).contains(&paused_alpha) || !(0.0..=1.0).contains(&corner_alpha) {
                anyhow::bail!("hub paused_alpha and corner_alpha must be between 0 and 1");
            }
        }
        match cfg.tiles.background {
            Some(Background::Media { alpha }) => {
                if !(0.0..=1.0).contains(&alpha) {
                    anyhow::bail!("background alpha must be between 0 and 1");
                }
                if hub_has_art {
                    anyhow::bail!("the art shows once: hub = media or background = media, not both");
                }
            }
            Some(Background::None) => {}
            None => {
                cfg.tiles.background = Some(if hub_has_art {
                    Background::None
                } else {
                    Background::Media { alpha: default_background_alpha() }
                });
            }
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::Units;

    #[test]
    fn converts_between_degrees_only() {
        assert_eq!(Units::Fahrenheit.convert(29.0, "°C"), (84.2, "°F".to_string()));
        assert_eq!(Units::Celsius.convert(212.0, "°F"), (100.0, "°C".to_string()));
        assert_eq!(Units::Fahrenheit.convert(50.0, "°F"), (50.0, "°F".to_string()));
        assert_eq!(Units::Celsius.convert(42.0, "%"), (42.0, "%".to_string()));
    }
}
