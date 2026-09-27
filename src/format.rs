//! The configuration file: one YAML file drives the panel. It names its
//! parts (colour schemes, layouts, tiles, pages, playlists) and refers to
//! them by name; `load` checks it and resolves it into the model.
//!
//! A page puts a tile in each region of its layout it wants filled, by the
//! tile's name or inline. Its palette is the `default` scheme, then the
//! page's scheme, then the tile's scheme, then the tile's own `colors`. With
//! no playlists, every page plays in file order; with no schedule, the first
//! playlist always plays.

use crate::canvas::{HEIGHT, WIDTH};
use crate::config::{
    Alert, ArtCacheConfig, FrameConfig, HomeAssistantConfig, PageData, SpotifyConfig, TileSpec, Units, WeatherConfig,
    When,
};
use crate::model::{self, Layer, Model};
use crate::palette::{Overrides, Palette};
use crate::secrets::{SecretRef, Secrets};
use anyhow::{bail, Context, Result};
use embedded_graphics::{prelude::*, primitives::Rectangle};
use indexmap::IndexMap;
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct File {
    /// WLED host, "host" or "host:port".
    pub target: String,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Gamma applied to pictures before they are sent: WLED does not
    /// correct streamed frames, so sRGB pictures need 2.2.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// The unit every temperature is shown in.
    #[serde(default)]
    pub temperature: Units,
    /// A copy of the board's gap file, for previews; relative to this file.
    #[serde(default = "default_gaps")]
    pub gaps: PathBuf,
    /// The secrets file, names to values, relative to this file.
    #[serde(default = "default_secrets")]
    pub secrets: PathBuf,
    #[serde(default)]
    pub sources: Sources,
    #[serde(default)]
    pub art_cache: ArtCacheConfig,
    /// A JPEG kept at the album cover on show; relative to this file.
    pub art_file: Option<PathBuf>,
    /// Run `open` on the art file after each change (macOS Preview).
    #[serde(default)]
    pub art_open: bool,
    /// Colour schemes by name; `default` applies to every tile.
    #[serde(default)]
    pub schemes: IndexMap<String, Overrides>,
    /// Layouts by name: regions of the panel by name.
    #[serde(default)]
    pub layouts: IndexMap<String, IndexMap<String, Region>>,
    /// Tiles by name, for pages to place.
    #[serde(default)]
    pub tiles: IndexMap<String, Tile>,
    /// How long a page shows when it does not say, in seconds.
    #[serde(default = "default_page_seconds")]
    pub page_seconds: f32,
    /// Pages by name.
    pub pages: IndexMap<String, Page>,
    /// Playlists by name: pages shown in turn, looping.
    #[serde(default)]
    pub playlists: IndexMap<String, Vec<String>>,
    /// The first rule that matches picks the playlist; none matching sends
    /// nothing, so the board falls back to its presets.
    #[serde(default)]
    pub schedule: Vec<Rule>,
    /// While an entity is in its state, the panel pulses with its label.
    #[serde(default)]
    pub alerts: Vec<Alert>,
    /// Where an alert centres its label.
    #[serde(default = "default_alert_area")]
    pub alert_area: Area,
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

fn default_secrets() -> PathBuf {
    "secrets.yaml".into()
}

fn default_page_seconds() -> f32 {
    10.0
}

fn default_alert_area() -> Area {
    Area { x: 21, y: 21, width: 22, height: 22 }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub weather: Option<WeatherConfig>,
    pub home_assistant: Option<HomeAssistant>,
    /// Takes precedence over a Home Assistant media player.
    pub spotify: Option<SpotifyConfig>,
    /// A folder of pictures for picture tiles.
    pub pictures: Option<Pictures>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistant {
    /// e.g. "http://homeassistant.local:8123"
    pub url: String,
    /// A long-lived access token (profile page, Security tab), kept in the
    /// secrets file.
    pub token: SecretRef,
    /// The media_player entity for now_playing and art tiles, when there
    /// is no Spotify.
    pub media_player: Option<String>,
    #[serde(default = "default_ha_refresh")]
    pub refresh_seconds: u64,
}

fn default_ha_refresh() -> u64 {
    10
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pictures {
    /// Relative to this file.
    #[serde(default = "default_pictures_dir")]
    pub dir: PathBuf,
    /// Each picture, in seconds.
    #[serde(default = "default_pictures_seconds")]
    pub seconds: f32,
    /// Random order, reshuffled at each start; else by file name.
    #[serde(default)]
    pub shuffle: bool,
}

fn default_pictures_dir() -> PathBuf {
    "frame".into()
}

fn default_pictures_seconds() -> f32 {
    10.0
}

/// A rectangle of the panel, in pixels.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A region of a layout: a rectangle, and where it stacks. Regions are drawn
/// in ascending `z`, those with the same `z` in the layout's order.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub z: i32,
}

impl Area {
    fn rect(&self) -> Rectangle {
        Rectangle::new(Point::new(self.x as i32, self.y as i32), Size::new(self.width, self.height))
    }

    fn check(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            bail!("width and height must be positive");
        }
        let past = |at: u32, len: u32, max: u32| at.checked_add(len).is_none_or(|end| end > max);
        if past(self.x, self.width, WIDTH) || past(self.y, self.height, HEIGHT) {
            bail!("must fit the {WIDTH}x{HEIGHT} panel");
        }
        Ok(())
    }
}

impl Region {
    fn area(&self) -> Area {
        Area { x: self.x, y: self.y, width: self.width, height: self.height }
    }
}

/// A tile: its kind and settings, and optionally a scheme and its own
/// colours over the page's.
#[derive(Debug, Clone)]
pub struct Tile {
    pub spec: TileSpec,
    pub scheme: Option<String>,
    pub colors: Overrides,
}

/// Kinds without settings of their own, which serde would let take any key.
const UNIT_KINDS: [&str; 4] = ["date", "weather", "now_playing", "blank"];

impl<'de> Deserialize<'de> for Tile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut map = serde_json::Map::deserialize(d)?;
        let scheme = match map.remove("scheme") {
            Some(v) => Some(String::deserialize(v).map_err(D::Error::custom)?),
            None => None,
        };
        let colors = match map.remove("colors") {
            Some(v) => Overrides::deserialize(v).map_err(D::Error::custom)?,
            None => Overrides::default(),
        };
        if let Some(kind) = map.get("kind").and_then(|k| k.as_str()).filter(|k| UNIT_KINDS.contains(k)) {
            if let Some(stray) = map.keys().find(|k| *k != "kind") {
                return Err(D::Error::custom(format!("unknown field `{stray}` for kind {kind}")));
            }
        }
        let spec = TileSpec::deserialize(serde_json::Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self { spec, scheme, colors })
    }
}

impl JsonSchema for Tile {
    fn schema_name() -> Cow<'static, str> {
        "Tile".into()
    }

    /// The kind's own schema, with `scheme` and `colors` beside the
    /// settings of every kind.
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = TileSpec::json_schema(generator);
        let colors = generator.subschema_for::<Overrides>();
        let scheme = json_schema!({
            "description": "A colour scheme's name, over the page's.",
            "type": "string"
        });
        if let Some(kinds) = schema.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
            for kind in kinds {
                if let Some(props) = kind.get_mut("properties").and_then(|v| v.as_object_mut()) {
                    props.insert("scheme".into(), scheme.clone().into());
                    props.insert("colors".into(), colors.clone().into());
                }
            }
        }
        schema
    }
}

/// A tile on a page: a named tile's name, or a tile spelt out.
#[derive(Debug, Clone)]
pub enum TileRef {
    Name(String),
    Inline(Tile),
}

impl JsonSchema for TileRef {
    fn schema_name() -> Cow<'static, str> {
        "TileRef".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let tile = generator.subschema_for::<Tile>();
        json_schema!({
            "description": "A tile's name from `tiles`, or a tile spelt out.",
            "anyOf": [{"type": "string"}, tile]
        })
    }
}

impl<'de> Deserialize<'de> for TileRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(name) => Ok(TileRef::Name(name)),
            v => Tile::deserialize(v).map(TileRef::Inline).map_err(D::Error::custom),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// The layout's name.
    pub layout: String,
    /// How long the page shows; `page_seconds` when left out.
    pub seconds: Option<f32>,
    /// A scheme over the default one for every tile on the page.
    pub scheme: Option<String>,
    /// The tile in each region, by region name. Regions left out stay
    /// empty.
    #[serde(default)]
    pub tiles: IndexMap<String, TileRef>,
    /// Values laid over the live data while the page shows.
    #[serde(default)]
    pub data: PageData,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub playlist: String,
    /// Left out, the rule always matches.
    pub when: Option<When>,
}

/// The JSON Schema of the file, draft 7 for the widest editor support.
pub fn schema() -> String {
    let generator = schemars::generate::SchemaSettings::draft07().into_generator();
    let mut schema = generator.into_root_schema_for::<File>();
    schema.insert("title".into(), "panel-ddp configuration".into());
    serde_json::to_string_pretty(&schema).expect("a schema serialises") + "\n"
}

/// Reads a YAML configuration file and resolves it.
pub fn load(path: &Path) -> Result<Model> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading config {}", path.display()))?;
    let file: File =
        serde_yaml_ng::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(""));
    let secrets = if file.secrets.is_relative() { dir.join(&file.secrets) } else { file.secrets.clone() };
    let mut model = file.into_model(dir).with_context(|| format!("config {}", path.display()))?;
    model.files = vec![path.to_path_buf(), secrets];
    Ok(model)
}

impl File {
    fn into_model(mut self, dir: &Path) -> Result<Model> {
        self.check_settings()?;
        let pages = self.pages()?;
        let (playlists, schedule) = self.playlists(&pages)?;
        self.check_sources(&pages)?;
        let rel = |p: PathBuf| if p.is_relative() { dir.join(p) } else { p };
        if let Some(w) = self.sources.weather.as_mut() {
            w.units.get_or_insert(self.temperature);
        }
        if let Some(sp) = self.sources.spotify.as_mut() {
            sp.token_file = rel(std::mem::take(&mut sp.token_file));
        }
        let mut secrets = Secrets::new(rel(self.secrets.clone()));
        let home_assistant = match self.sources.home_assistant.take() {
            Some(h) => Some(HomeAssistantConfig {
                token: secrets.get(&h.token).context("sources.home_assistant.token")?,
                url: h.url,
                media_player: h.media_player,
                refresh_seconds: h.refresh_seconds,
            }),
            None => None,
        };
        let pictures = self.sources.pictures.take();
        let frame = FrameConfig {
            dir: rel(pictures.as_ref().map_or_else(default_pictures_dir, |p| p.dir.clone())),
            seconds: pictures.as_ref().map_or_else(default_pictures_seconds, |p| p.seconds),
            shuffle: pictures.as_ref().is_some_and(|p| p.shuffle),
            alpha: 1.0,
        };
        let mut art_cache = self.art_cache;
        art_cache.dir = rel(art_cache.dir);
        Ok(Model {
            target: self.target,
            fps: self.fps,
            gamma: self.gamma,
            temperature: self.temperature,
            gaps: rel(self.gaps),
            weather: self.sources.weather,
            spotify: self.sources.spotify,
            home_assistant,
            art_cache,
            frame,
            alerts: self.alerts,
            alert_area: self.alert_area.rect(),
            pages,
            playlists,
            schedule,
            art_file: self.art_file.map(rel),
            art_open: self.art_open,
            files: Vec::new(),
        })
    }

    /// The checks on single settings.
    fn check_settings(&self) -> Result<()> {
        if self.fps == 0 {
            bail!("fps must be positive");
        }
        if !(0.5..=5.0).contains(&self.gamma) {
            bail!("gamma must be between 0.5 and 5");
        }
        if self.page_seconds.is_nan() || self.page_seconds <= 0.0 {
            bail!("page_seconds must be positive");
        }
        if self.art_cache.max_megabytes.is_nan() || self.art_cache.max_megabytes < 0.0 {
            bail!("art_cache.max_megabytes must be 0 or more");
        }
        if let Some(p) = &self.sources.pictures {
            if p.seconds.is_nan() || p.seconds <= 0.0 {
                bail!("sources.pictures.seconds must be positive");
            }
        }
        self.alert_area.check().context("alert_area")?;
        for a in &self.alerts {
            if a.label.chars().count() > 11 || a.label.is_empty() {
                bail!("alert {}: the label is one to eleven characters", a.entity);
            }
            if a.pulse_seconds.is_nan() || a.pulse_seconds <= 0.0 {
                bail!("alert {}: pulse_seconds must be positive", a.entity);
            }
        }
        for (name, layout) in &self.layouts {
            for (region, r) in layout {
                r.area().check().with_context(|| format!("layout {name}, region {region}"))?;
            }
        }
        for (name, t) in &self.tiles {
            t.spec.check().with_context(|| format!("tile {name}"))?;
        }
        Ok(())
    }

    fn scheme(&self, name: &str) -> Result<&Overrides> {
        self.schemes.get(name).with_context(|| format!("no scheme named {name}{}", known(self.schemes.keys())))
    }

    /// Every page resolved into layers, in file order.
    fn pages(&self) -> Result<Vec<model::Page>> {
        if self.pages.is_empty() {
            bail!("there are no pages");
        }
        let base = match self.schemes.get("default") {
            Some(o) => Palette::default().with(o),
            None => Palette::default(),
        };
        self.pages
            .iter()
            .map(|(name, p)| self.page(name, p, base).with_context(|| format!("page {name}")))
            .collect()
    }

    fn page(&self, name: &str, p: &Page, base: Palette) -> Result<model::Page> {
        let layout = self
            .layouts
            .get(&p.layout)
            .with_context(|| format!("no layout named {}{}", p.layout, known(self.layouts.keys())))?;
        let seconds = p.seconds.unwrap_or(self.page_seconds);
        if seconds.is_nan() || seconds <= 0.0 {
            bail!("seconds must be positive");
        }
        for (entity, v) in &p.data.sensors {
            if v.state.is_some() == v.sweep.is_some() {
                bail!("sensor {entity} takes either state or sweep");
            }
        }
        if let Some(region) = p.tiles.keys().find(|r| !layout.contains_key(*r)) {
            bail!("layout {} has no region {region}{}", p.layout, known(layout.keys()));
        }
        let page_palette = match &p.scheme {
            Some(s) => base.with(self.scheme(s)?),
            None => base,
        };
        // Drawing order: by z, then the layout's order.
        let mut regions: Vec<(usize, &String, &Region)> =
            layout.iter().enumerate().map(|(i, (n, r))| (i, n, r)).collect();
        regions.sort_by_key(|&(i, _, r)| (r.z, i));
        let mut layers = Vec::new();
        for (_, region, r) in regions {
            let Some(tile) = p.tiles.get(region) else { continue };
            let tile = match tile {
                TileRef::Name(t) => self
                    .tiles
                    .get(t)
                    .with_context(|| format!("region {region}: no tile named {t}{}", known(self.tiles.keys())))?,
                TileRef::Inline(t) => {
                    t.spec.check().with_context(|| format!("region {region}"))?;
                    t
                }
            };
            let mut palette = page_palette;
            if let Some(s) = &tile.scheme {
                palette = palette.with(self.scheme(s).with_context(|| format!("region {region}"))?);
            }
            layers.push(Layer { area: r.area().rect(), tile: tile.spec.clone(), palette: palette.with(&tile.colors) });
        }
        Ok(model::Page { name: name.to_string(), seconds: Some(seconds), layers, data: p.data.clone() })
    }

    /// The playlists and the schedule, by index, with their defaults.
    fn playlists(&self, pages: &[model::Page]) -> Result<(Vec<model::Playlist>, Vec<model::Rule>)> {
        let playlists: Vec<model::Playlist> = if self.playlists.is_empty() {
            vec![model::Playlist { name: "pages".into(), pages: (0..pages.len()).collect() }]
        } else {
            self.playlists
                .iter()
                .map(|(name, list)| {
                    if list.is_empty() {
                        bail!("playlist {name} has no pages");
                    }
                    let pages = list
                        .iter()
                        .map(|p| {
                            self.pages.get_index_of(p).with_context(|| {
                                format!("playlist {name}: no page named {p}{}", known(self.pages.keys()))
                            })
                        })
                        .collect::<Result<_>>()?;
                    Ok(model::Playlist { name: name.clone(), pages })
                })
                .collect::<Result<_>>()?
        };
        let schedule = if self.schedule.is_empty() {
            vec![model::Rule { playlist: 0, when: None }]
        } else {
            self.schedule
                .iter()
                .map(|r| {
                    let playlist = playlists.iter().position(|l| l.name == r.playlist).with_context(|| {
                        format!("schedule: no playlist named {}{}", r.playlist, known(playlists.iter().map(|l| &l.name)))
                    })?;
                    Ok(model::Rule { playlist, when: r.when.clone() })
                })
                .collect::<Result<_>>()?
        };
        Ok((playlists, schedule))
    }

    /// Each tile's source is configured, or the page it is on supplies the
    /// data itself, as a demo does.
    fn check_sources(&self, pages: &[model::Page]) -> Result<()> {
        let s = &self.sources;
        let media = s.spotify.is_some() || s.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some());
        for page in pages {
            let d = &page.data;
            for layer in &page.layers {
                let missing = match &layer.tile {
                    TileSpec::Weather if s.weather.is_none() && d.weather.is_none() => Some("sources.weather"),
                    TileSpec::Sensor { entity, .. } | TileSpec::Progress { entity, .. }
                        if s.home_assistant.is_none() && !d.sensors.contains_key(entity) =>
                    {
                        Some("sources.home_assistant")
                    }
                    TileSpec::NowPlaying | TileSpec::Art { .. } if !media && d.cover.is_none() => {
                        Some("sources.spotify or sources.home_assistant.media_player")
                    }
                    TileSpec::Picture { .. } if s.pictures.is_none() => Some("sources.pictures"),
                    _ => None,
                };
                if let Some(source) = missing {
                    bail!("page {}: a {} tile needs {source}", page.name, kind_name(&layer.tile));
                }
            }
            if d.picture.is_some() && s.pictures.is_none() {
                bail!("page {}: data.picture needs sources.pictures", page.name);
            }
        }
        if s.home_assistant.is_none() {
            for a in &self.alerts {
                if !pages.iter().any(|p| p.data.sensors.contains_key(&a.entity)) {
                    bail!("alert {}: needs sources.home_assistant, or a page giving its state", a.entity);
                }
            }
        }
        Ok(())
    }
}

fn kind_name(t: &TileSpec) -> &'static str {
    match t {
        TileSpec::Clock { .. } => "clock",
        TileSpec::Date => "date",
        TileSpec::Weather => "weather",
        TileSpec::Sensor { .. } => "sensor",
        TileSpec::Progress { .. } => "progress",
        TileSpec::NowPlaying => "now_playing",
        TileSpec::Art { .. } => "art",
        TileSpec::Picture { .. } => "picture",
        TileSpec::Blank => "blank",
    }
}

/// " (known: a, b)" for an error about a name, or nothing when none are.
fn known<'a>(names: impl Iterator<Item = &'a String>) -> String {
    let v: Vec<&str> = names.map(String::as_str).collect();
    if v.is_empty() {
        String::new()
    } else {
        format!(" (known: {})", v.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::TileSpec;
    use crate::palette::{Palette, Rgba};

    fn load(yaml: &str) -> anyhow::Result<crate::model::Model> {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("panel-ddp-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("c{}.yaml", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        std::fs::write(&path, yaml).unwrap();
        super::load(&path)
    }

    const BASE: &str = r##"
target: wled.local
sources:
  spotify: {client_id: abc}
schemes:
  default: {text: "#101010"}
  dim: {text: "#202020", label: "#303030"}
layouts:
  classic:
    hub: {x: 21, y: 21, width: 22, height: 22, z: 1}
    top_left: {x: 2, y: 2, width: 24, height: 24}
    back: {x: 0, y: 0, width: 64, height: 64, z: -1}
tiles:
  clock: {kind: clock, seconds: ring}
  cover: {kind: art, shape: square, alpha: 0.5, idle: none, scheme: dim, colors: {label: "#404040"}}
pages:
  home:
    layout: classic
    tiles: {top_left: clock, back: cover, hub: {kind: date}}
  night:
    layout: classic
    seconds: 3
    scheme: dim
    tiles: {top_left: clock}
"##;

    #[test]
    fn resolves_names_order_and_palettes() {
        let m = load(BASE).unwrap();
        assert_eq!(m.pages.len(), 2);
        let home = &m.pages[0];
        assert_eq!(home.seconds, Some(10.0), "page_seconds by default");
        let kinds: Vec<&str> = home.layers.iter().map(|l| super::kind_name(&l.tile)).collect();
        assert_eq!(kinds, ["art", "clock", "date"], "by z, then the layout's order");
        let text = |l: &crate::model::Layer| l.palette.text;
        assert_eq!(text(&home.layers[1]), Rgba::rgb(0x10, 0x10, 0x10), "default scheme");
        assert_eq!(text(&home.layers[0]), Rgba::rgb(0x20, 0x20, 0x20), "the tile's scheme");
        assert_eq!(home.layers[0].palette.label, Rgba::rgb(0x40, 0x40, 0x40), "its own colours last");
        assert_eq!(text(&m.pages[1].layers[0]), Rgba::rgb(0x20, 0x20, 0x20), "the page's scheme");
        assert_eq!(m.pages[1].layers[0].palette.track, Palette::default().track);
        assert!(matches!(home.layers[0].tile, TileSpec::Art { .. }));
        assert_eq!(m.playlists.len(), 1, "every page, in order");
        assert_eq!(m.playlists[0].pages, [0, 1]);
        assert_eq!(m.schedule.len(), 1);
    }

    #[test]
    fn playlists_and_schedule_by_name() {
        let m = load(&format!(
            "{BASE}playlists:\n  day: [home]\n  night: [night, home]\nschedule:\n  - playlist: night\n    when: {{from: \"23:00\", to: \"07:00\"}}\n  - playlist: day\n"
        ))
        .unwrap();
        assert_eq!(m.playlists[1].pages, [1, 0]);
        assert_eq!(m.schedule[0].playlist, 1);
        assert!(m.schedule[0].when.is_some() && m.schedule[1].when.is_none());
    }

    #[test]
    fn mistakes_are_named() {
        let cases = [
            (BASE.replace("layout: classic\n    seconds", "layout: big\n    seconds"), "no layout named big"),
            (BASE.replace("top_left: clock}", "top_left: clok}"), "no tile named clok"),
            (BASE.replace("scheme: dim\n    tiles", "scheme: dark\n    tiles"), "no scheme named dark"),
            (BASE.replace("{top_left: clock}", "{middle: clock}"), "has no region middle"),
            (BASE.replace("seconds: ring", "second: ring"), "unknown field `second`"),
            (BASE.replace("{kind: date}", "{kind: date, label: x}"), "unknown field `label`"),
            (BASE.replace("{kind: date}", "{kind: weather}"), "needs sources.weather"),
            (BASE.replace("width: 64", "width: 65"), "must fit"),
            (format!("{BASE}schedule:\n  - playlist: nights\n"), "no playlist named nights"),
        ];
        for (yaml, want) in cases {
            let err = format!("{:#}", load(&yaml).unwrap_err());
            assert!(err.contains(want), "{want:?} not in {err:?}");
        }
    }

    #[test]
    fn the_committed_schema_is_current() {
        let committed = include_str!("../panel-ddp.schema.json");
        assert!(
            committed == super::schema(),
            "panel-ddp.schema.json is out of date: target/release/panel-ddp schema > panel-ddp.schema.json"
        );
    }

    fn validator() -> jsonschema::Validator {
        let schema: serde_json::Value = serde_json::from_str(&super::schema()).unwrap();
        jsonschema::validator_for(&schema).unwrap()
    }

    fn as_json(yaml: &str) -> serde_json::Value {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn the_schema_accepts_what_loads_and_refuses_typos() {
        let v = validator();
        for yaml in [BASE.to_string(), format!("{BASE}playlists:\n  day: [home]\nschedule:\n  - {{playlist: day, when: {{days: [mon], from: \"06:30\"}}}}\n")] {
            let errors: Vec<String> = v.iter_errors(&as_json(&yaml)).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{errors:?}");
        }
        for bad in [
            BASE.replace("seconds: ring", "second: ring"),
            BASE.replace("{kind: date}", "{kind: date, label: x}"),
            BASE.replace("{kind: date}", "{kind: dates}"),
            BASE.replace("target: wled.local", "targets: wled.local"),
            BASE.replace("\"#101010\"", "\"#1010\""),
            format!("{BASE}schedule:\n  - {{playlist: day, when: {{from: \"25:00\"}}}}\n"),
        ] {
            assert!(!v.is_valid(&as_json(&bad)), "accepted: {bad}");
        }
    }

    #[test]
    fn the_token_comes_from_the_secrets_file() {
        let yaml = BASE.replace(
            "sources:\n",
            "secrets: test-secrets.yaml\nsources:\n  home_assistant: {url: \"http://ha\", token: {secret: ha_token}}\n",
        );
        let dir = std::env::temp_dir().join(format!("panel-ddp-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(format!("{:#}", load(&yaml).unwrap_err()).contains("test-secrets.yaml"));
        std::fs::write(dir.join("test-secrets.yaml"), "ha_token: abc123\n").unwrap();
        assert_eq!(load(&yaml).unwrap().home_assistant.unwrap().token, "abc123");
        assert!(load(&BASE.replace("sources:\n", "sources:\n  home_assistant: {url: x, token: abc}\n")).is_err(), "no literal tokens");
    }

    #[test]
    fn page_data_stands_in_for_sources() {
        let yaml = BASE.replace("{kind: date}", "{kind: weather}").replace(
            "tiles: {top_left: clock, back: cover, hub: {kind: weather}}",
            "tiles: {top_left: clock, back: cover, hub: {kind: weather}}\n    data: {weather: {code: 0, temperature: 20}, cover: 0}",
        );
        load(&yaml).unwrap();
    }
}
