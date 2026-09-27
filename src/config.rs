//! The building blocks of a configuration, whatever the file's format: the
//! sources, the alerts, the tile kinds and the made-up data a page may lay
//! over the live one. `legacy` reads them from today's TOML and JSON files,
//! and `model` is what a file resolves into.

use crate::palette::Rgba;
use serde::Deserialize;
use std::path::PathBuf;

/// Values a page lays over the live data, for demos and for pinning a page
/// to something the sources do not report.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageData {
    pub weather: Option<WeatherData>,
    /// By entity id: a fixed state, or a sweep from one number to another
    /// over the page's time.
    #[serde(default)]
    pub sensors: std::collections::BTreeMap<String, SensorData>,
    /// A cover from the art cache, newest first (0 is the newest), played.
    pub cover: Option<usize>,
    /// The `[frame]` picture, by position, for a `frame` background.
    pub picture: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherData {
    /// WMO weather code.
    pub code: u16,
    #[serde(default = "yes")]
    pub is_day: bool,
    /// In the configured `temperature` unit.
    pub temperature: f32,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorData {
    pub state: Option<String>,
    /// From, to: the value moves linearly across the page, rounded.
    pub sweep: Option<[f64; 2]>,
    pub unit: Option<String>,
}

/// Picture frame mode: a folder of pictures shown in turn.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameConfig {
    /// Relative to the config file.
    #[serde(default = "default_frame_dir")]
    pub dir: PathBuf,
    /// Each picture, in seconds.
    #[serde(default = "default_frame_seconds")]
    pub seconds: f32,
    /// Random order, reshuffled at each start; else by file name.
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default = "default_frame_alpha")]
    pub alpha: f32,
}

fn default_frame_dir() -> PathBuf {
    "frame".into()
}

fn default_frame_seconds() -> f32 {
    10.0
}

fn default_frame_alpha() -> f32 {
    1.0
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self { dir: default_frame_dir(), seconds: default_frame_seconds(), shuffle: false, alpha: default_frame_alpha() }
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
    /// Spotify for its largest. The `art_file` is then written from
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
    /// Left out, the loader fills it in.
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

pub(crate) fn default_dot_size() -> u32 {
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
