//! Today's configuration files: `dashboard.toml` and the same in JSON
//! (`demo.json`), read as they always were and resolved into the model the
//! drawing code works from. `[tiles]` alone makes a single page shown for
//! good; `pages` a loop of timed ones. On every page the background comes
//! first, then the five regions in ascending `z`.

use crate::canvas::{HEIGHT, WIDTH};
use crate::config::{
    default_corner_alpha, default_dot_size, default_paused_alpha, Alert, ArtCacheConfig, ArtShape, FrameConfig,
    HomeAssistantConfig, Idle, PageData, Seconds, SpotifyConfig, TileSpec, Units, WeatherConfig,
};
use crate::model::{self, Layer, Model};
use crate::palette::{Overrides, Palette};
use anyhow::{Context, Result};
use embedded_graphics::{prelude::*, primitives::Rectangle};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Reads a TOML file, or JSON for a `.json` one, and resolves it.
pub fn load(path: &Path) -> Result<Model> {
    Ok(Config::read(path)?.into_model())
}

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
    /// Where each tile and the hub sit, and which draws over which.
    #[serde(default)]
    pub regions: Regions,
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
    /// The folder and pacing for `panel-ddp frame`.
    #[serde(default)]
    pub frame: FrameConfig,
    /// Dashboard pages shown in turn, each for its own time. When there are
    /// any, `run` loops through them instead of showing `[tiles]`.
    #[serde(default)]
    pub pages: Vec<Page>,
    /// A JPEG kept at the album cover on show, black when there is none;
    /// relative to the config file. Removed when the run ends.
    pub art_file: Option<PathBuf>,
    /// Run `open` on the art file after each change, so macOS Preview shows
    /// and re-reads it.
    #[serde(default)]
    pub art_open: bool,
}

/// One page of a looping dashboard: a layout, how long it stays, and
/// optionally made-up data laid over whatever the sources report.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub seconds: f32,
    #[serde(default = "blank_tile")]
    pub top_left: TileEntry,
    #[serde(default = "blank_tile")]
    pub top_right: TileEntry,
    #[serde(default = "blank_tile")]
    pub bottom_left: TileEntry,
    #[serde(default = "blank_tile")]
    pub bottom_right: TileEntry,
    #[serde(default = "Tiles::default_hub")]
    pub hub: HubEntry,
    /// Unlike `[tiles]`, a page shows no background unless it names one.
    #[serde(default = "no_background")]
    pub background: Background,
    #[serde(default)]
    pub data: PageData,
}

fn blank_tile() -> TileEntry {
    TileSpec::Blank.into()
}

fn no_background() -> Background {
    Background::None
}

impl Page {
    pub fn tiles(&self) -> Tiles {
        Tiles {
            top_left: self.top_left.clone(),
            top_right: self.top_right.clone(),
            bottom_left: self.bottom_left.clone(),
            bottom_right: self.bottom_right.clone(),
            hub: self.hub.clone(),
            background: Some(self.background.clone()),
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

/// The five places on the screen: four tiles and the hub. Regions with the
/// same `z` are drawn in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Hub,
}

impl Slot {
    pub const ALL: [Slot; 5] = [Slot::TopLeft, Slot::TopRight, Slot::BottomLeft, Slot::BottomRight, Slot::Hub];

    pub fn name(self) -> &'static str {
        match self {
            Slot::TopLeft => "top_left",
            Slot::TopRight => "top_right",
            Slot::BottomLeft => "bottom_left",
            Slot::BottomRight => "bottom_right",
            Slot::Hub => "hub",
        }
    }
}

/// A rectangle of the screen and where it stacks: regions are drawn in
/// ascending `z`, so a higher one covers a lower one where they overlap.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub z: i32,
}

impl Region {
    const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height, z: 0 }
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn rect(&self) -> Rectangle {
        Rectangle::new(Point::new(self.x as i32, self.y as i32), self.size())
    }
}

/// Where each tile and the hub sit. Unset, a region keeps its place in the
/// default layout: four 24x24 tiles two pixels in from the corners, and a
/// 22x22 hub in the middle, drawn over their inner corners.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regions {
    #[serde(default = "Regions::default_top_left")]
    pub top_left: Region,
    #[serde(default = "Regions::default_top_right")]
    pub top_right: Region,
    #[serde(default = "Regions::default_bottom_left")]
    pub bottom_left: Region,
    #[serde(default = "Regions::default_bottom_right")]
    pub bottom_right: Region,
    #[serde(default = "Regions::default_hub")]
    pub hub: Region,
}

impl Regions {
    fn default_top_left() -> Region {
        Region::new(2, 2, 24, 24)
    }
    fn default_top_right() -> Region {
        Region::new(38, 2, 24, 24)
    }
    fn default_bottom_left() -> Region {
        Region::new(2, 38, 24, 24)
    }
    fn default_bottom_right() -> Region {
        Region::new(38, 38, 24, 24)
    }
    fn default_hub() -> Region {
        Region::new(21, 21, 22, 22)
    }

    pub fn get(&self, slot: Slot) -> Region {
        match slot {
            Slot::TopLeft => self.top_left,
            Slot::TopRight => self.top_right,
            Slot::BottomLeft => self.bottom_left,
            Slot::BottomRight => self.bottom_right,
            Slot::Hub => self.hub,
        }
    }

    /// Every region in drawing order: by `z`, then in `Slot` order.
    pub fn in_order(&self) -> Vec<(Slot, Region)> {
        let mut v: Vec<(Slot, Region)> = Slot::ALL.iter().map(|&s| (s, self.get(s))).collect();
        v.sort_by_key(|&(s, r)| (r.z, s));
        v
    }

    fn validate(&self) -> Result<()> {
        for slot in Slot::ALL {
            let r = self.get(slot);
            if r.width == 0 || r.height == 0 {
                anyhow::bail!("{}: width and height must be positive", slot.name());
            }
            let past = |at: u32, len: u32, max: u32| at.checked_add(len).is_none_or(|end| end > max);
            if past(r.x, r.width, WIDTH) || past(r.y, r.height, HEIGHT) {
                anyhow::bail!("{}: must fit the {WIDTH}x{HEIGHT} panel", slot.name());
            }
        }
        Ok(())
    }
}

impl Default for Regions {
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

/// What each tile shows, and the hub in the middle.
#[derive(Debug, Clone, Deserialize)]
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
    /// The `[frame]` pictures in turn, at `[frame].seconds` each, blended
    /// over black at `alpha`.
    Frame {
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
    /// The tile in a corner slot; the hub is not a tile.
    pub fn tile(&self, slot: Slot) -> Option<&TileEntry> {
        match slot {
            Slot::TopLeft => Some(&self.top_left),
            Slot::TopRight => Some(&self.top_right),
            Slot::BottomLeft => Some(&self.bottom_left),
            Slot::BottomRight => Some(&self.bottom_right),
            Slot::Hub => None,
        }
    }

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

impl Config {
    /// The four corner tiles, in reading order.
    fn corner_tiles(&self) -> [&TileSpec; 4] {
        [&self.tiles.top_left.spec, &self.tiles.top_right.spec, &self.tiles.bottom_left.spec, &self.tiles.bottom_right.spec]
    }

    /// Whether something can feed the now_playing tile and the media hub.
    /// Spotify takes precedence over a Home Assistant media player.
    fn has_media_source(&self) -> bool {
        self.spotify.is_some() || self.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some())
    }

    fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        // TOML, or JSON for a `.json` file; the fields are the same.
        let mut cfg: Config = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            serde_json::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?
        } else {
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?
        };
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
        validate_tiles(&cfg.tiles).context("[tiles]")?;
        cfg.regions.validate().context("[regions]")?;
        for (i, page) in cfg.pages.iter().enumerate() {
            let n = i + 1;
            if page.seconds.is_nan() || page.seconds <= 0.0 {
                anyhow::bail!("page {n}: seconds must be positive");
            }
            validate_tiles(&page.tiles()).with_context(|| format!("page {n}"))?;
            for (entity, v) in &page.data.sensors {
                if v.state.is_some() == v.sweep.is_some() {
                    anyhow::bail!("page {n}: sensor {entity} takes either state or sweep");
                }
            }
        }
        // With pages, their data may stand in for the sources, so the
        // sources are only required for a plain [tiles] dashboard.
        let tiles = cfg.corner_tiles();
        if cfg.pages.is_empty() {
            if cfg.weather.is_none() && tiles.iter().any(|t| matches!(t, TileSpec::Weather)) {
                anyhow::bail!("a weather tile needs the [weather] table");
            }
            let needs_ha = tiles.iter().any(|t| matches!(t, TileSpec::Sensor { .. } | TileSpec::Progress { .. }))
                || !cfg.alerts.is_empty();
            if needs_ha && cfg.home_assistant.is_none() {
                anyhow::bail!("sensor and progress tiles, and alerts, need the [home_assistant] table");
            }
        }
        if cfg.frame.seconds.is_nan() || cfg.frame.seconds <= 0.0 || !(0.0..=1.0).contains(&cfg.frame.alpha) {
            anyhow::bail!("[frame] seconds must be positive and alpha between 0 and 1");
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
        if needs_player && !cfg.has_media_source() && cfg.pages.is_empty() {
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
            if cfg.frame.dir.is_relative() {
                cfg.frame.dir = dir.join(&cfg.frame.dir);
            }
            if let Some(f) = cfg.art_file.as_mut() {
                if f.is_relative() {
                    *f = dir.join(&*f);
                }
            }
        }
        // Last, after the tiles borrow ends: where the art goes when unset.
        if cfg.tiles.background.is_none() {
            let hub_has_art = matches!(cfg.tiles.hub.spec, HubSpec::Media { .. });
            cfg.tiles.background = Some(if hub_has_art {
                Background::None
            } else {
                Background::Media { alpha: default_background_alpha() }
            });
        }
        Ok(cfg)
    }
}

impl Config {
    fn into_model(self) -> Model {
        // `[colors]` for every tile, a tile's own `colors` over them.
        let base = Palette::default().with(&self.colors);
        let pages = if self.pages.is_empty() {
            vec![model::Page {
                name: "tiles".into(),
                seconds: None,
                layers: layers(&self.tiles, &self.regions, base),
                data: PageData::default(),
            }]
        } else {
            self.pages
                .iter()
                .enumerate()
                .map(|(i, p)| model::Page {
                    name: format!("page {}", i + 1),
                    seconds: Some(p.seconds),
                    layers: layers(&p.tiles(), &self.regions, base),
                    data: p.data.clone(),
                })
                .collect()
        };
        // Every page, in order, at any time.
        let playlists = vec![model::Playlist { name: "pages".into(), pages: (0..pages.len()).collect() }];
        let schedule = vec![model::Rule { playlist: 0, when: None }];
        Model {
            target: self.target,
            fps: self.fps,
            gamma: self.gamma,
            temperature: self.temperature,
            gaps: self.gaps,
            weather: self.weather,
            spotify: self.spotify,
            home_assistant: self.home_assistant,
            art_cache: self.art_cache,
            frame: self.frame,
            alerts: self.alerts,
            alert_area: self.regions.hub.rect(),
            pages,
            playlists,
            schedule,
            art_file: self.art_file,
            art_open: self.art_open,
        }
    }
}

/// A page's layers: the background across the panel, then the tiles and
/// the hub in their regions, in drawing order. The background is a square
/// art tile at its alpha that neither dims when paused nor ripples, or a
/// picture tile; the hub is an art tile or blank.
fn layers(tiles: &Tiles, regions: &Regions, base: Palette) -> Vec<Layer> {
    let panel = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT));
    let mut v = Vec::new();
    let background = match tiles.background {
        Some(Background::Media { alpha }) => Some(TileSpec::Art {
            shape: ArtShape::Square,
            spin: false,
            paused_alpha: 1.0,
            corner_alpha: 1.0,
            alpha,
            idle: Idle::None,
        }),
        Some(Background::Frame { alpha }) => Some(TileSpec::Picture { alpha }),
        Some(Background::None) | None => None,
    };
    if let Some(tile) = background {
        v.push(Layer { area: panel, tile, palette: base });
    }
    for (slot, region) in regions.in_order() {
        v.push(match tiles.tile(slot) {
            Some(e) => Layer { area: region.rect(), tile: e.spec.clone(), palette: base.with(&e.colors) },
            None => Layer { area: region.rect(), tile: tiles.hub.spec.to_tile(), palette: base.with(&tiles.hub.colors) },
        });
    }
    v
}

impl HubSpec {
    /// The hub as a tile: blank, or art at full alpha that ripples while
    /// there is none.
    fn to_tile(&self) -> TileSpec {
        match *self {
            HubSpec::Blank => TileSpec::Blank,
            HubSpec::Media { spin, shape, paused_alpha, corner_alpha } => {
                TileSpec::Art { shape, spin, paused_alpha, corner_alpha, alpha: 1.0, idle: Idle::Ripple }
            }
        }
    }
}

/// The checks that do not depend on which sources are configured.
fn validate_tiles(t: &Tiles) -> Result<()> {
    for e in [&t.top_left, &t.top_right, &t.bottom_left, &t.bottom_right] {
        match &e.spec {
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
            TileSpec::Picture { alpha } if !(0.0..=1.0).contains(alpha) => {
                anyhow::bail!("picture tile alpha must be between 0 and 1")
            }
            _ => {}
        }
    }
    let hub_has_art = matches!(t.hub.spec, HubSpec::Media { .. });
    if let HubSpec::Media { paused_alpha, corner_alpha, .. } = t.hub.spec {
        if !(0.0..=1.0).contains(&paused_alpha) || !(0.0..=1.0).contains(&corner_alpha) {
            anyhow::bail!("hub paused_alpha and corner_alpha must be between 0 and 1");
        }
    }
    match t.background {
        Some(Background::Media { alpha }) | Some(Background::Frame { alpha }) if !(0.0..=1.0).contains(&alpha) => {
            anyhow::bail!("background alpha must be between 0 and 1")
        }
        Some(Background::Media { .. }) if hub_has_art => {
            anyhow::bail!("the art shows once: hub = media or background = media, not both")
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_and_toml_read_the_same() {
        let dir = std::env::temp_dir().join(format!("panel-ddp-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let toml_path = dir.join("c.toml");
        let json_path = dir.join("c.json");
        std::fs::write(
            &toml_path,
            "target = \"x\"\n[[pages]]\nseconds = 2\ntop_left = { kind = \"clock\", colors = { track = \"#ffffff10\" } }\ndata = { sensors = { \"sensor.a\" = { sweep = [0, 100] } } }\n",
        )
        .unwrap();
        std::fs::write(
            &json_path,
            r##"{"target": "x", "pages": [{"seconds": 2, "top_left": {"kind": "clock", "colors": {"track": "#ffffff10"}}, "data": {"sensors": {"sensor.a": {"sweep": [0, 100]}}}}]}"##,
        )
        .unwrap();
        let a = super::Config::read(&toml_path).unwrap();
        let b = super::Config::read(&json_path).unwrap();
        assert_eq!(format!("{:?}", a.pages), format!("{:?}", b.pages));
        let bad = dir.join("bad.json");
        std::fs::write(&bad, r#"{"target": "x", "pages": [{"seconds": 2, "top_left": {"kind": "date", "bogus": 1}}]}"#).unwrap();
        assert!(super::Config::read(&bad).is_err(), "stray keys are refused in JSON too");
    }

    #[test]
    fn regions_default_to_the_classic_layout_and_stack_by_z() {
        use super::{Regions, Slot};
        let r = Regions::default();
        let order: Vec<Slot> = r.in_order().into_iter().map(|(s, _)| s).collect();
        assert_eq!(order, Slot::ALL, "same z: slot order, hub last");
        assert_eq!((r.hub.x, r.hub.y, r.hub.width, r.hub.height), (21, 21, 22, 22));

        let r: Regions = toml::from_str("hub = { x = 0, y = 0, width = 64, height = 64, z = -1 }\ntop_left = { x = 2, y = 2, width = 24, height = 24, z = 5 }").unwrap();
        let order: Vec<Slot> = r.in_order().into_iter().map(|(s, _)| s).collect();
        assert_eq!(order, [Slot::Hub, Slot::TopRight, Slot::BottomLeft, Slot::BottomRight, Slot::TopLeft]);
        assert_eq!(r.top_right, Regions::default().top_right, "unset regions keep their default");
    }

    #[test]
    fn regions_must_fit_the_panel() {
        let dir = std::env::temp_dir().join(format!("panel-ddp-regions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (bad, why) in [
            ("hub = { x = 50, y = 0, width = 20, height = 10 }", "past the right edge"),
            ("hub = { x = 0, y = 0, width = 0, height = 10 }", "zero width"),
            ("hub = { x = 0, y = 0, width = 10 }", "missing height"),
            ("hub = { x = 0, y = 0, width = 10, height = 10, depth = 1 }", "unknown key"),
        ] {
            let path = dir.join("r.toml");
            std::fs::write(&path, format!("target = \"x\"\n[regions]\n{bad}\n")).unwrap();
            assert!(super::Config::read(&path).is_err(), "{why} is refused");
        }
    }

}
