//! The building blocks of a configuration: the sources, the alerts, the
//! tile kinds, the schedule's rules and the made-up data a page may lay
//! over the live one. `format` reads them from the YAML file (and `legacy`
//! from older ones, to migrate them); `model` is what a file resolves into.

use crate::palette::Rgba;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Values a page lays over the live data, for demos and for pinning a page
/// to something the sources do not report.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageData {
    pub weather: Option<WeatherData>,
    /// By entity id: a fixed state, or a sweep from one number to another
    /// over the page's time.
    #[serde(default)]
    pub sensors: std::collections::BTreeMap<String, SensorData>,
    /// By entity id or series name: the values a sparkline of it draws,
    /// oldest first, whatever its `hours`.
    #[serde(default)]
    pub series: std::collections::BTreeMap<String, Vec<f64>>,
    /// A cover from the art cache, newest first (0 is the newest), played.
    pub cover: Option<usize>,
    /// The picture from `sources.pictures`, by position, for picture tiles.
    pub picture: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SensorData {
    pub state: Option<String>,
    /// From, to: the value moves linearly across the page, rounded.
    pub sweep: Option<[f64; 2]>,
    pub unit: Option<String>,
}

/// A folder of pictures shown in turn by picture tiles.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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
}

fn default_frame_dir() -> PathBuf {
    "frame".into()
}

fn default_frame_seconds() -> f32 {
    10.0
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self { dir: default_frame_dir(), seconds: default_frame_seconds(), shuffle: false }
    }
}


#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    pub entity: String,
    /// The state that raises the alert.
    #[serde(default = "default_alert_state")]
    pub state: String,
    /// Shown on the alert area: up to four characters large in its middle,
    /// up to eleven small along its top.
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
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistantConfig {
    /// e.g. "http://homeassistant.local:8123"
    pub url: String,
    /// A long-lived access token (profile page, bottom).
    pub token: String,
    /// The media_player entity for now_playing and art tiles.
    pub media_player: Option<String>,
    #[serde(default = "default_refresh_seconds")]
    pub refresh_seconds: u64,
}

/// The HTTP endpoint the values of sparkline series are pushed to.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// The address and port to listen on.
    pub listen: String,
    /// What a request must carry as `Authorization: Bearer`, when set.
    pub token: Option<String>,
}

pub(crate) fn default_listen() -> String {
    "127.0.0.1:4049".into()
}

fn default_refresh_seconds() -> u64 {
    10
}

fn default_max() -> f64 {
    100.0
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
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
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Seconds {
    /// The ring fills clockwise from twelve.
    Ring,
    /// A single dot travels round the ring, like a second hand.
    #[default]
    Dot,
}

/// A text tile's font, by the width and height of a character in pixels.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub enum TextSize {
    #[serde(rename = "4x6")]
    S4X6,
    #[serde(rename = "5x7")]
    S5X7,
    #[serde(rename = "5x8")]
    S5X8,
    #[serde(rename = "6x9")]
    S6X9,
    #[default]
    #[serde(rename = "6x10")]
    S6X10,
    #[serde(rename = "6x12")]
    S6X12,
    #[serde(rename = "6x13")]
    S6X13,
    #[serde(rename = "7x13")]
    S7X13,
    #[serde(rename = "7x14")]
    S7X14,
    #[serde(rename = "8x13")]
    S8X13,
    #[serde(rename = "9x15")]
    S9X15,
    #[serde(rename = "9x18")]
    S9X18,
    #[serde(rename = "10x20")]
    S10X20,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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
    /// One line of text, centred; it scrolls when wider than the region.
    Text {
        text: String,
        #[serde(default)]
        size: TextSize,
    },
    /// Sky icon and temperature; needs sources.weather.
    Weather,
    /// A Home Assistant entity's state under a label; needs sources.home_assistant.
    Sensor {
        entity: String,
        label: String,
        /// Overrides the entity's unit_of_measurement; "" hides it.
        unit: Option<String>,
        /// Decimal places for a numeric state; fewer are used if it does not fit.
        decimals: Option<u8>,
    },
    /// A Home Assistant entity's numeric state as a bar under the value,
    /// full at `max`; needs sources.home_assistant.
    Progress {
        entity: String,
        label: String,
        #[serde(default = "default_max")]
        max: f64,
        /// Decimal places for the value; 0 unless set.
        #[serde(default)]
        decimals: u8,
    },
    /// A line, low at the bottom of the region and high at the top, of
    /// either a Home Assistant `entity`'s numeric history (needs
    /// sources.home_assistant) or a `series` pushed over HTTP (needs
    /// sources.http).
    Sparkline {
        entity: Option<String>,
        /// The name the values are pushed under, at /series/NAME: letters,
        /// digits, `.`, `_` and `-`.
        series: Option<String>,
        /// How far back an entity's line goes.
        #[serde(default = "default_hours")]
        hours: u32,
        /// The line's colour; the `accent` colour unless set.
        line: Option<Rgba>,
        /// A dot at the latest value, in this colour; none unless set.
        dot: Option<Rgba>,
    },
    /// Artist and title scrolling; needs sources.spotify or a Home Assistant media_player.
    NowPlaying,
    /// The album art; needs sources.spotify or a Home Assistant media_player.
    Art {
        #[serde(default)]
        shape: ArtShape,
        /// Turn the art while playing.
        #[serde(default)]
        spin: bool,
        /// The art's alpha while paused.
        #[serde(default = "default_paused_alpha")]
        paused_alpha: f32,
        /// The alpha of the corners outside the circle in the faded shape.
        #[serde(default = "default_corner_alpha")]
        corner_alpha: f32,
        /// The art's alpha over what is drawn under it.
        #[serde(default = "one")]
        alpha: f32,
        /// What shows while there is no art.
        #[serde(default)]
        idle: Idle,
    },
    /// The picture from `sources.pictures` due now, at `alpha` over what is
    /// drawn under it.
    Picture {
        #[serde(default = "one")]
        alpha: f32,
    },
    /// Nothing.
    Blank,
}

fn default_hours() -> u32 {
    24
}

fn one() -> f32 {
    1.0
}

impl TileSpec {
    /// The checks a tile's own settings must pass, whatever the sources.
    pub fn check(&self) -> anyhow::Result<()> {
        match self {
            TileSpec::Progress { max, label, .. } if max.is_nan() || *max <= 0.0 => {
                anyhow::bail!("progress tile {label}: max must be positive")
            }
            TileSpec::Clock { dot_size, .. } if !(1..=12).contains(dot_size) => {
                anyhow::bail!("clock dot_size must be between 1 and 12")
            }
            TileSpec::Art { paused_alpha, corner_alpha, alpha, .. }
                if ![paused_alpha, corner_alpha, alpha].iter().all(|a| (0.0..=1.0).contains(*a)) =>
            {
                anyhow::bail!("art tile alphas must be between 0 and 1")
            }
            TileSpec::Sparkline { entity, series, .. } if entity.is_some() == series.is_some() => {
                anyhow::bail!("a sparkline takes either entity or series")
            }
            TileSpec::Sparkline { hours, .. } if !(1..=720).contains(hours) => {
                anyhow::bail!("sparkline hours must be between 1 and 720")
            }
            TileSpec::Sparkline { series: Some(name), .. }
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) =>
            {
                anyhow::bail!("sparkline series {name:?}: a name is letters, digits, '.', '_' and '-'")
            }
            TileSpec::Picture { alpha } if !(0.0..=1.0).contains(alpha) => {
                anyhow::bail!("picture tile alpha must be between 0 and 1")
            }
            _ => Ok(()),
        }
    }
}

/// What an art tile shows while there is no art.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Idle {
    /// A slow ripple from the middle.
    #[default]
    Ripple,
    /// Nothing.
    None,
}

pub(crate) fn default_paused_alpha() -> f32 {
    0.4
}

pub(crate) fn default_corner_alpha() -> f32 {
    0.3
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
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

/// When a schedule rule applies: on some days, between two times of day.
/// Left out, a part does not limit: no `days` is every day, no `from` is
/// from midnight, no `to` is until midnight. A `to` earlier than `from`
/// runs past midnight. Days are those of the moment, local time.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct When {
    #[serde(default)]
    pub days: Vec<Day>,
    pub from: Option<TimeOfDay>,
    pub to: Option<TimeOfDay>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Day {
    fn weekday(self) -> chrono::Weekday {
        use chrono::Weekday::*;
        match self {
            Day::Mon => Mon,
            Day::Tue => Tue,
            Day::Wed => Wed,
            Day::Thu => Thu,
            Day::Fri => Fri,
            Day::Sat => Sat,
            Day::Sun => Sun,
        }
    }
}

/// A time of day, "HH:MM", 00:00 to 23:59.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOfDay {
    /// Minutes since midnight.
    pub minutes: u32,
}

impl TimeOfDay {
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = || format!("{s:?}: a time of day is \"HH:MM\", 00:00 to 23:59");
        let (h, m) = s.split_once(':').ok_or_else(bad)?;
        if h.len() != 2 || m.len() != 2 {
            return Err(bad());
        }
        let (h, m): (u32, u32) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
        if h > 23 || m > 59 {
            return Err(bad());
        }
        Ok(Self { minutes: h * 60 + m })
    }
}

impl JsonSchema for TimeOfDay {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeOfDay".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A time of day, \"HH:MM\", 00:00 to 23:59, local time.",
            "type": "string",
            "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$"
        })
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{:02}:{:02}", self.minutes / 60, self.minutes % 60))
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        TimeOfDay::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl When {
    pub fn matches<Tz: chrono::TimeZone>(&self, now: &chrono::DateTime<Tz>) -> bool {
        use chrono::{Datelike, Timelike};
        if !self.days.is_empty() && !self.days.iter().any(|d| d.weekday() == now.weekday()) {
            return false;
        }
        let t = now.hour() * 60 + now.minute();
        match (self.from, self.to) {
            (None, None) => true,
            (Some(f), None) => t >= f.minutes,
            (None, Some(e)) => t < e.minutes,
            (Some(f), Some(e)) if f <= e => f.minutes <= t && t < e.minutes,
            (Some(f), Some(e)) => t >= f.minutes || t < e.minutes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Units;

    #[test]
    fn when_matches_days_and_windows() {
        use super::{Day, TimeOfDay, When};
        use chrono::{FixedOffset, TimeZone};
        // 2026-09-25 is a Friday.
        let at = |d: u32, h: u32, m: u32| FixedOffset::east_opt(0).unwrap().with_ymd_and_hms(2026, 9, d, h, m, 0).unwrap();
        let t = |s: &str| Some(TimeOfDay::parse(s).unwrap());
        assert!(When::default().matches(&at(25, 3, 0)));
        let night = When { days: vec![], from: t("23:00"), to: t("07:00") };
        assert!(night.matches(&at(25, 23, 0)) && night.matches(&at(26, 6, 59)));
        assert!(!night.matches(&at(26, 7, 0)) && !night.matches(&at(25, 22, 59)));
        let office = When { days: vec![Day::Mon, Day::Fri], from: t("09:00"), to: t("17:30") };
        assert!(office.matches(&at(25, 9, 0)) && office.matches(&at(25, 17, 29)));
        assert!(!office.matches(&at(25, 17, 30)) && !office.matches(&at(26, 12, 0)));
        assert!(When { days: vec![], from: t("12:00"), to: None }.matches(&at(26, 23, 59)));
        assert!(!When { days: vec![], from: None, to: t("12:00") }.matches(&at(26, 12, 0)));
        for bad in ["24:00", "9:00", "12:60", "noon"] {
            assert!(TimeOfDay::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn converts_between_degrees_only() {
        assert_eq!(Units::Fahrenheit.convert(29.0, "°C"), (84.2, "°F".to_string()));
        assert_eq!(Units::Celsius.convert(212.0, "°F"), (100.0, "°C".to_string()));
        assert_eq!(Units::Fahrenheit.convert(50.0, "°F"), (50.0, "°F".to_string()));
        assert_eq!(Units::Celsius.convert(42.0, "%"), (42.0, "%".to_string()));
    }
}
