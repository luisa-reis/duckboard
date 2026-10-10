//! The configuration file: one YAML file drives the panel. It names its
//! parts (colour schemes, layouts, tiles, pages, playlists) and refers to
//! them by name; `load` checks it and resolves it into the model.
//!
//! A page puts a tile in each region of its layout it wants filled, by the
//! tile's name or inline. Its palette is the `default` scheme, then the
//! page's scheme, then the tile's scheme, then the tile's own `colors`. With
//! no playlists, every page plays in file order; with no schedule, the first
//! playlist always plays.

use crate::canvas::{DEFAULT_HEIGHT, DEFAULT_WIDTH};
use crate::config::{
    default_listen, is_name, Alert, ArtCacheConfig, CommandConfig, FrameConfig, HomeAssistantConfig, HttpConfig, PageData, SpotifyConfig, TileSpec,
    Units, WeatherConfig, When,
};
use crate::model::{self, Layer, Model};
use crate::palette::{Overrides, Palette};
use crate::command::Feed;
use crate::data::{records, MAX_ROWS};
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
    /// The panel's width in pixels; WLED must be set up as a matrix of the
    /// same size.
    #[serde(default = "default_width")]
    pub width: u32,
    /// The panel's height in pixels.
    #[serde(default = "default_height")]
    pub height: u32,
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
    /// Where an alert centres its label; a 22x22 square in the middle of
    /// the panel when left out.
    pub alert_area: Option<Area>,
}

fn default_width() -> u32 {
    DEFAULT_WIDTH
}

fn default_height() -> u32 {
    DEFAULT_HEIGHT
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

impl File {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// The alert area: as given, or 22x22 in the middle of the panel.
    fn alert_area(&self) -> Area {
        self.alert_area.unwrap_or(Area {
            x: self.width.saturating_sub(22) / 2,
            y: self.height.saturating_sub(22) / 2,
            width: self.width.min(22),
            height: self.height.min(22),
        })
    }
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
    /// An HTTP endpoint taking the values of chart series.
    pub http: Option<Http>,
    /// Commands run again and again, each for a table's rows or a chart's
    /// series: what it prints, as JSON.
    #[serde(default)]
    pub commands: Vec<CommandSource>,
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Http {
    /// The address and port to listen on. The default takes requests from
    /// this machine only; "0.0.0.0:4049" takes them from the network.
    #[serde(default = "default_listen")]
    pub listen: String,
    /// A token every request must then carry, as the header
    /// `Authorization: Bearer TOKEN`, kept in the secrets file. Without
    /// one, anything that reaches the port can change the series.
    pub token: Option<SecretRef>,
}

/// A command whose output, JSON, is a table's rows or a chart's series. It
/// runs in the config file's directory, when the panel starts and then
/// every `every` seconds, with the panel's own rights.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandSource {
    /// The program and its arguments, each on its own: no shell reads
    /// them. For a pipeline, run `["sh", "-c", "…"]`.
    pub run: Vec<String>,
    /// The `data` name of the table its output fills: a JSON array of
    /// objects, a row each, as `sqlite3 -json` and `duckdb -json` print.
    pub table: Option<String>,
    /// The name of the series its output is: a JSON array of numbers, or
    /// of rows with the numbers in one column.
    pub series: Option<String>,
    /// For a series from rows of several columns: the one to take.
    pub column: Option<String>,
    /// Seconds from the end of one run to the start of the next.
    #[serde(default = "default_command_every")]
    pub every: u64,
    /// Seconds a run may take; it is stopped after that, and what was
    /// there stays.
    #[serde(default = "default_command_timeout")]
    pub timeout: u64,
    /// Secrets for its environment, by variable: `PGPASSWORD: {secret:
    /// pg_password}`. It has the panel's environment besides.
    #[serde(default)]
    pub env: IndexMap<String, SecretRef>,
}

fn default_command_every() -> u64 {
    60
}

fn default_command_timeout() -> u64 {
    30
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

    fn check(&self, panel: Size) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            bail!("width and height must be positive");
        }
        let past = |at: u32, len: u32, max: u32| at.checked_add(len).is_none_or(|end| end > max);
        if past(self.x, self.width, panel.width) || past(self.y, self.height, panel.height) {
            bail!("must fit the {}x{} panel", panel.width, panel.height);
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
    pub body: Body,
    pub scheme: Option<String>,
    pub colors: Overrides,
}

/// What a tile is: one of the kinds that draw, or a table of them.
#[derive(Debug, Clone)]
pub enum Body {
    Spec(TileSpec),
    Table(Table),
}

/// Rows of tiles, stacked from the top of the region the table is in. The
/// table's `scheme` and `colors` apply to its tiles, under their own.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub rows: Vec<Row>,
    /// The name rows are pushed under, at /tables/NAME, for the table's
    /// repeat row: letters, digits, `.`, `_` and `-`.
    pub data: Option<String>,
    /// Pixels between two rows.
    #[serde(default)]
    pub gap: u32,
}

/// A row of a table: its height, and the tiles on it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Row {
    /// In pixels.
    pub height: u32,
    /// Lays the row out once for each row pushed as the table's `data`, as
    /// many as fit the region; its tiles take their values by `column`.
    /// Only the last row repeats.
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub tiles: Vec<Cell>,
}

/// A tile on a row, placed from the row's top left corner.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    /// Pixels from the row's left edge.
    pub x: u32,
    /// Pixels from the row's top edge.
    #[serde(default)]
    pub y: u32,
    pub width: u32,
    /// The rest of the row's height unless set.
    pub height: Option<u32>,
    /// A tile's name from `tiles`, or a tile spelt out; not a table.
    pub tile: TileRef,
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
        let body = if map.get("kind").and_then(|k| k.as_str()) == Some("table") {
            map.remove("kind");
            Body::Table(Table::deserialize(serde_json::Value::Object(map)).map_err(D::Error::custom)?)
        } else {
            Body::Spec(TileSpec::deserialize(serde_json::Value::Object(map)).map_err(D::Error::custom)?)
        };
        Ok(Self { body, scheme, colors })
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
        let mut table = Table::json_schema(generator);
        table.insert("description".into(), "Rows of tiles, stacked from the top of the region.".into());
        if let Some(props) = table.get_mut("properties").and_then(|v| v.as_object_mut()) {
            props.insert("kind".into(), serde_json::json!({"type": "string", "const": "table"}));
        }
        if let Some(required) = table.get_mut("required").and_then(|v| v.as_array_mut()) {
            required.push("kind".into());
        }
        if let Some(kinds) = schema.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
            kinds.push(table.into());
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
    Inline(Box<Tile>),
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
            v => Tile::deserialize(v).map(|t| TileRef::Inline(Box::new(t))).map_err(D::Error::custom),
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
        let alert_area = self.alert_area().rect();
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
        let http = match self.sources.http.take() {
            Some(h) => Some(HttpConfig {
                token: h.token.map(|t| secrets.get(&t).context("sources.http.token")).transpose()?,
                listen: h.listen,
            }),
            None => None,
        };
        if http.as_ref().is_some_and(|h| h.token.as_deref() == Some("")) {
            bail!("sources.http.token: the secret is empty");
        }
        let commands = std::mem::take(&mut self.sources.commands)
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let at = || format!("sources.commands, entry {}", i + 1);
                let feed = match (c.table, c.series) {
                    (Some(name), None) => Feed::Table(name),
                    (None, Some(name)) => Feed::Series(name),
                    _ => bail!("{}: takes either table or series", at()),
                };
                if c.run.is_empty() || c.run[0].trim().is_empty() {
                    bail!("{}: run needs a program", at());
                }
                if c.every == 0 || c.timeout == 0 {
                    bail!("{}: every and timeout must be at least 1", at());
                }
                if c.column.is_some() && matches!(feed, Feed::Table(_)) {
                    bail!("{}: column is for a series", at());
                }
                let env = c
                    .env
                    .iter()
                    .map(|(name, secret)| Ok((name.clone(), secrets.get(secret).with_context(|| format!("{}: env {name}", at()))?)))
                    .collect::<Result<_>>()?;
                Ok(CommandConfig { run: c.run, feed, column: c.column, every: c.every, timeout: c.timeout, env, dir: dir.to_path_buf() })
            })
            .collect::<Result<Vec<_>>>()?;
        for c in &commands {
            let (what, name, drawn) = match &c.feed {
                Feed::Table(name) => ("table", name, pages.iter().flat_map(|p| &p.layers).any(|l| l.record.as_ref().is_some_and(|r| r.0 == *name))),
                Feed::Series(name) => ("series", name, pages.iter().flat_map(|p| &p.layers).any(|l| l.tile.pushed_series() == Some(name))),
            };
            if !drawn {
                bail!("sources.commands: nothing draws the {what} {name}");
            }
        }
        let pictures = self.sources.pictures.take();
        let frame = FrameConfig {
            dir: rel(pictures.as_ref().map_or_else(default_pictures_dir, |p| p.dir.clone())),
            seconds: pictures.as_ref().map_or_else(default_pictures_seconds, |p| p.seconds),
            shuffle: pictures.as_ref().is_some_and(|p| p.shuffle),
        };
        let mut art_cache = self.art_cache;
        art_cache.dir = rel(art_cache.dir);
        Ok(Model {
            target: self.target,
            width: self.width,
            height: self.height,
            fps: self.fps,
            gamma: self.gamma,
            temperature: self.temperature,
            gaps: rel(self.gaps),
            weather: self.sources.weather,
            spotify: self.sources.spotify,
            home_assistant,
            http,
            commands,
            art_cache,
            frame,
            alerts: self.alerts,
            alert_area,
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
        if !(1..=1024).contains(&self.width) || !(1..=1024).contains(&self.height) {
            bail!("width and height must be between 1 and 1024");
        }
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
        self.alert_area().check(self.size()).context("alert_area")?;
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
                r.area().check(self.size()).with_context(|| format!("layout {name}, region {region}"))?;
            }
        }
        for (name, t) in &self.tiles {
            if let Body::Spec(spec) = &t.body {
                spec.check().with_context(|| format!("tile {name}"))?;
            }
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
        if let Some((entity, _)) = p.data.series.iter().find(|(_, v)| v.is_empty() || v.iter().any(|x| !x.is_finite())) {
            bail!("series {entity} takes one or more numbers");
        }
        for (table, rows) in &p.data.tables {
            let rows = serde_json::Value::Array(rows.iter().cloned().map(serde_json::Value::Object).collect());
            records(&rows).map_err(|e| anyhow::anyhow!("data.tables.{table}: {e}"))?;
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
            let (tile, palette) = self.tile(tile, page_palette).with_context(|| format!("region {region}"))?;
            match &tile.body {
                Body::Spec(spec) if !spec.columns().is_empty() => {
                    bail!("region {region}: column is for a tile on a table's repeat row")
                }
                Body::Spec(spec) => layers.push(Layer { area: r.area().rect(), tile: spec.clone(), palette, record: None }),
                Body::Table(table) => {
                    let cells = self.table(table, r.area(), palette).with_context(|| format!("region {region}"))?;
                    layers.extend(cells);
                }
            }
        }
        Ok(model::Page { name: name.to_string(), seconds: Some(seconds), layers, data: p.data.clone() })
    }

    /// The tile a reference stands for, checked when it is spelt out, and
    /// the palette it draws in over `under`: its scheme, then its colours.
    fn tile<'a>(&'a self, tile: &'a TileRef, under: Palette) -> Result<(&'a Tile, Palette)> {
        let tile = match tile {
            TileRef::Name(t) => {
                self.tiles.get(t).with_context(|| format!("no tile named {t}{}", known(self.tiles.keys())))?
            }
            TileRef::Inline(t) => {
                if let Body::Spec(spec) = &t.body {
                    spec.check()?;
                }
                t.as_ref()
            }
        };
        let palette = match &tile.scheme {
            Some(s) => under.with(self.scheme(s)?),
            None => under,
        };
        Ok((tile, palette.with(&tile.colors)))
    }

    /// A table in `area` as layers, one for each of its tiles: the rows
    /// stacked from the area's top, each tile placed from its row's corner.
    /// A repeat row is laid out as many times as fit under the rows before
    /// it, each time for the next of the pushed rows.
    fn table(&self, table: &Table, area: Area, palette: Palette) -> Result<Vec<Layer>> {
        if let Some(name) = table.data.as_ref().filter(|n| !is_name(n)) {
            bail!("data {name:?}: a name is letters, digits, '.', '_' and '-'");
        }
        let mut layers = Vec::new();
        let mut top = 0u32;
        for (i, row) in table.rows.iter().enumerate() {
            let n = i + 1;
            let fits = |top: u32| top.checked_add(row.height).filter(|bottom| row.height > 0 && *bottom <= area.height);
            if fits(top).is_none() {
                bail!("row {n}: the rows must each have a height and fit the region's {} pixels", area.height);
            }
            let data = match (row.repeat, &table.data) {
                (false, _) => None,
                (true, Some(name)) if n == table.rows.len() => Some(name),
                (true, Some(_)) => bail!("row {n}: the repeat row must be the last"),
                (true, None) => bail!("row {n}: a repeat row needs the table's data"),
            };
            // A row of its own, or one for each pushed row there is room for.
            let mut record = 0;
            while let Some(bottom) = fits(top) {
                for (j, cell) in row.tiles.iter().enumerate() {
                    let at = || format!("row {n}, tile {}", j + 1);
                    let (tile, palette) = self.tile(&cell.tile, palette).with_context(at)?;
                    let Body::Spec(spec) = &tile.body else { bail!("{}: a table cannot hold a table", at()) };
                    if !spec.columns().is_empty() && data.is_none() {
                        bail!("{}: column is for a tile on a table's repeat row", at());
                    }
                    let height = cell.height.unwrap_or(row.height.saturating_sub(cell.y));
                    let past = |from: u32, len: u32, max: u32| from.checked_add(len).is_none_or(|end| end > max);
                    if cell.width == 0 || height == 0 || past(cell.x, cell.width, area.width) || past(cell.y, height, row.height) {
                        bail!("{}: must fit the row, {} wide and {} high", at(), area.width, row.height);
                    }
                    let corner = Point::new((area.x + cell.x) as i32, (area.y + top + cell.y) as i32);
                    layers.push(Layer {
                        area: Rectangle::new(corner, Size::new(cell.width, height)),
                        tile: spec.clone(),
                        palette,
                        record: data.map(|name| (name.clone(), record)),
                    });
                }
                top = bottom.saturating_add(table.gap);
                record += 1;
                if data.is_none() || record == MAX_ROWS {
                    break;
                }
            }
        }
        Ok(layers)
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
        // Whether a command fills the table, or is the series, of that name.
        let fed = |name: &String, table: bool| {
            s.commands.iter().any(|c| if table { c.table.as_ref() } else { c.series.as_ref() } == Some(name))
        };
        for page in pages {
            let d = &page.data;
            for layer in &page.layers {
                let missing = match &layer.tile {
                    TileSpec::Weather if s.weather.is_none() && d.weather.is_none() => Some("sources.weather"),
                    TileSpec::Sensor { entity, .. }
                    | TileSpec::Progress { entity, .. }
                    | TileSpec::BulletChart { entity: Some(entity), .. }
                        if s.home_assistant.is_none() && !d.sensors.contains_key(entity) =>
                    {
                        Some("sources.home_assistant")
                    }
                    t if matches!(t.chart(), Some((Some(entity), _, _)) if s.home_assistant.is_none() && !d.series.contains_key(entity)) =>
                    {
                        Some("sources.home_assistant")
                    }
                    t if matches!(t.pushed_series(), Some(name) if s.http.is_none() && !d.series.contains_key(name) && !fed(name, false)) =>
                    {
                        Some("sources.http or a command in sources.commands")
                    }
                    TileSpec::NowPlaying | TileSpec::Art { .. } if !media && d.cover.is_none() => {
                        Some("sources.spotify or sources.home_assistant.media_player")
                    }
                    TileSpec::Picture { .. } if s.pictures.is_none() => Some("sources.pictures"),
                    _ => None,
                };
                let missing = match &layer.record {
                    Some((table, _)) if s.http.is_none() && !d.tables.contains_key(table) && !fed(table, true) => {
                        Some("sources.http or a command in sources.commands")
                    }
                    _ => missing,
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
        TileSpec::Text { .. } => "text",
        TileSpec::Weather => "weather",
        TileSpec::Sensor { .. } => "sensor",
        TileSpec::Progress { .. } => "progress",
        TileSpec::LineChart { .. } => "line_chart",
        TileSpec::AreaChart { .. } => "area_chart",
        TileSpec::BarChart { .. } => "bar_chart",
        TileSpec::BulletChart { .. } => "bullet_chart",
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
    use crate::config::{Align, Overflow, TextSize, TileSpec};
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
    fn text_tile_sizes() {
        let page = |tile: &str| format!("target: t\nlayouts: {{l: {{a: {{x: 0, y: 0, width: 64, height: 20}}}}}}\npages: {{p: {{layout: l, tiles: {{a: {tile}}}}}}}\n");
        let size = |tile: &str| match load(&page(tile)).unwrap().pages[0].layers[0].tile {
            TileSpec::Text { size, .. } => size,
            ref other => panic!("{other:?}"),
        };
        assert_eq!(size("{kind: text, text: hello}"), TextSize::S6X10, "the default");
        let placed = |tile: &str| match load(&page(tile)).unwrap().pages[0].layers[0].tile {
            TileSpec::Text { align, overflow, .. } => (align, overflow),
            ref other => panic!("{other:?}"),
        };
        assert_eq!(placed("{kind: text, text: hello}"), (Align::Center, Overflow::Scroll), "the defaults");
        assert_eq!(placed("{kind: text, text: hello, align: right, overflow: truncate}"), (Align::Right, Overflow::Truncate));
        assert!(load(&page("{kind: text, text: hello, align: middle}")).is_err());
        assert_eq!(size("{kind: text, text: hello, size: 10x20}"), TextSize::S10X20);
        assert!(load(&page("{kind: text, text: hello, size: 3x5}")).is_err(), "not a size");
        assert!(load(&page("{kind: text}")).is_err(), "no text");
    }

    #[test]
    fn line_chart_needs_its_history() {
        let page = |tile: &str, data: &str| format!("target: t\nlayouts: {{l: {{a: {{x: 0, y: 0, width: 64, height: 20}}}}}}\npages: {{p: {{layout: l, tiles: {{a: {tile}}}, data: {data}}}}}\n");
        let m = load(&page("{kind: line_chart, entity: sensor.a, dot: '#ff0000'}", "{series: {sensor.a: [1, 2]}}")).unwrap();
        match m.pages[0].layers[0].tile {
            TileSpec::LineChart { hours, line, dot, .. } => {
                assert_eq!((hours, line, dot), (24, None, Some(Rgba::rgb(255, 0, 0))));
            }
            ref other => panic!("{other:?}"),
        }
        assert_eq!(m.series(), [("sensor.a".to_string(), 24)]);
        let err = |tile: &str, data: &str| format!("{:#}", load(&page(tile, data)).unwrap_err());
        assert!(err("{kind: line_chart, entity: sensor.a}", "{}").contains("sources.home_assistant"));
        assert!(err("{kind: line_chart, entity: sensor.a}", "{series: {sensor.a: []}}").contains("series sensor.a"));
        assert!(err("{kind: line_chart, entity: sensor.a, hours: 0}", "{series: {sensor.a: [1]}}").contains("hours"));
        assert!(err("{kind: line_chart, series: power}", "{}").contains("sources.http"));
        assert!(err("{kind: line_chart}", "{}").contains("one of entity, series or column"));
        assert!(err("{kind: line_chart, entity: sensor.a, series: a}", "{}").contains("one of entity, series or column"));
        assert!(err("{kind: line_chart, series: a/b}", "{}").contains("a name is"));
        let m = load(&page("{kind: line_chart, series: power}", "{series: {power: [1]}}")).unwrap();
        assert_eq!((m.series(), m.pushed()), (vec![], vec!["power".to_string()]));
        let area = "{kind: area_chart, series: power, area: '#00ff0080', area_bottom: '#00ff0000'}";
        let m = load(&page(area, "{series: {power: [1]}}")).unwrap();
        assert_eq!(m.pushed(), ["power"]);
        assert!(matches!(m.pages[0].layers[0].tile, TileSpec::AreaChart { area: Some(_), area_bottom: Some(_), line: None, .. }));
        assert!(err("{kind: area_chart, series: power}", "{}").contains("area_chart tile needs sources.http"));
        assert!(err("{kind: area_chart}", "{}").contains("one of entity, series or column"));
        let m = load(&page("{kind: bar_chart, entity: sensor.a, hours: 48, last: '#ffffff'}", "{series: {sensor.a: [1]}}")).unwrap();
        assert_eq!(m.series(), [("sensor.a".to_string(), 48)]);
        assert!(matches!(m.pages[0].layers[0].tile, TileSpec::BarChart { width: 2, gap: 1, bar: None, last: Some(_), .. }));
        assert!(err("{kind: bar_chart, series: power, width: 0}", "{series: {power: [1]}}").contains("width is 1 to 16"));
        assert!(err("{kind: bar_chart, series: power}", "{}").contains("bar_chart tile needs sources.http"));
        let m = load(&page("{kind: line_chart, series: power}", "{}").replace("target: t", "target: t\nsources: {http: {}}")).unwrap();
        assert_eq!(m.http.unwrap().listen, "127.0.0.1:4049");
    }

    #[test]
    fn a_table_is_a_layer_for_each_of_its_tiles() {
        let yaml = |rows: &str| {
            format!(
                "target: t\nschemes: {{dim: {{text: '#202020', label: '#303030'}}}}\n\
                 layouts: {{l: {{a: {{x: 4, y: 10, width: 40, height: 30}}}}}}\n\
                 tiles:\n  name: {{kind: text, text: hi, colors: {{label: '#505050'}}}}\n\
                 \x20 inner: {{kind: table, rows: []}}\n\
                 \x20 sheet: {{kind: table, gap: 2, scheme: dim, colors: {{accent: '#404040'}}, rows: {rows}}}\n\
                 pages: {{p: {{layout: l, tiles: {{a: sheet}}}}}}\n"
            )
        };
        let rows = "[{height: 8, tiles: [{x: 0, width: 10, tile: name}, {x: 12, y: 2, width: 28, height: 5, tile: {kind: date}}]}, \
                    {height: 6}, {height: 12, tiles: [{x: 1, y: 3, width: 39, tile: name}]}]";
        let m = load(&yaml(rows)).unwrap();
        let layers = &m.pages[0].layers;
        let places: Vec<(i32, i32, u32, u32)> =
            layers.iter().map(|l| (l.area.top_left.x, l.area.top_left.y, l.area.size.width, l.area.size.height)).collect();
        assert_eq!(places, [(4, 10, 10, 8), (16, 12, 28, 5), (5, 31, 39, 9)], "rows stack with the gap; tiles from their row");
        assert!(matches!(layers[1].tile, TileSpec::Date));
        let p = layers[0].palette;
        assert_eq!((p.text, p.accent, p.label), (Rgba::rgb(0x20, 0x20, 0x20), Rgba::rgb(0x40, 0x40, 0x40), Rgba::rgb(0x50, 0x50, 0x50)));
        assert_eq!(layers[1].palette.label, Rgba::rgb(0x30, 0x30, 0x30), "the table's scheme, under a tile's own colours");
        let err = |rows: &str| format!("{:#}", load(&yaml(rows)).unwrap_err());
        assert!(err("[{height: 8, tiles: [{x: 0, width: 10, tile: nope}]}]").contains("region a: row 1, tile 1: no tile named nope"));
        assert!(err("[{height: 8, tiles: [{x: 0, width: 10, tile: inner}]}]").contains("a table cannot hold a table"));
        assert!(err("[{height: 8, tiles: [{x: 31, width: 10, tile: name}]}]").contains("row 1, tile 1: must fit the row, 40 wide and 8 high"));
        assert!(err("[{height: 8, tiles: [{x: 0, y: 4, width: 10, height: 5, tile: name}]}]").contains("must fit the row"));
        assert!(err("[{height: 20}, {height: 9}]").contains("row 2: the rows must each have a height and fit the region's 30 pixels"));
        assert!(err("[{height: 0}]").contains("row 1"));
        assert!(err("[{height: 8, tiles: [{x: 0, width: 10, tile: {kind: clock, dot_size: 0}}]}]").contains("dot_size"));
        assert!(err("[{height: 8, wide: 1}]").contains("unknown field"));
    }

    #[test]
    fn a_repeat_row_is_laid_out_for_each_pushed_row() {
        let yaml = |table: &str, tile: &str, data: &str| {
            format!(
                "target: t\nlayouts: {{l: {{a: {{x: 0, y: 4, width: 40, height: 30}}}}}}\n\
                 tiles: {{sheet: {table}}}\npages: {{p: {{layout: l, tiles: {{a: {tile}}}, data: {data}}}}}\n"
            )
        };
        let table = "{kind: table, data: rooms, gap: 1, rows: [{height: 7, tiles: [{x: 0, width: 9, tile: {kind: text, text: hi}}]}, \
                     {height: 5, repeat: true, tiles: [{x: 0, width: 20, tile: {kind: text, column: room}}, \
                     {x: 21, width: 19, tile: {kind: bar_chart, column: history}}]}]}";
        let data = "{tables: {rooms: [{room: Hall, history: [1, 2]}]}}";
        let m = load(&yaml(table, "sheet", data)).unwrap();
        let layers = &m.pages[0].layers;
        let rows: Vec<(i32, Option<usize>)> = layers.iter().map(|l| (l.area.top_left.y, l.record.as_ref().map(|r| r.1))).collect();
        let expected = [(4, None), (12, Some(0)), (12, Some(0)), (18, Some(1)), (18, Some(1)), (24, Some(2)), (24, Some(2))];
        assert_eq!(rows, expected, "as many as fit under the header: 5 and a gap each in the 22 left");
        assert_eq!(layers[1].record.as_ref().unwrap().0, "rooms");
        let columns: Vec<(String, &str)> = m.table_columns()["rooms"].clone().into_iter().collect();
        assert_eq!(columns, [("history".to_string(), "numbers"), ("room".to_string(), "text")]);
        let err = |table: &str, tile: &str, data: &str| format!("{:#}", load(&yaml(table, tile, data)).unwrap_err());
        assert!(err(table, "sheet", "{}").contains("needs sources.http"), "or the page's data");
        assert!(err(table, "sheet", "{tables: {rooms: [{room: {a: 1}}]}}").contains("data.tables.rooms: row 1, column room"));
        assert!(err(&table.replace("data: rooms, ", ""), "sheet", data).contains("row 2: a repeat row needs the table's data"));
        assert!(err(&table.replace("{height: 7,", "{height: 7, repeat: true,"), "sheet", data).contains("row 1: the repeat row must be the last"));
        assert!(err(&table.replace("repeat: true, ", ""), "sheet", data).contains("row 2, tile 1: column is for a tile on a table's repeat row"));
        assert!(err(&table.replace("data: rooms", "data: a/b"), "sheet", data).contains("a name is"));
        assert!(err(table, "{kind: text, column: room}", data).contains("region a: column is for a tile on a table's repeat row"));
        assert!(err(table, "{kind: text}", data).contains("either text or column"));
        assert!(err(table, "{kind: text, text: a, column: b}", data).contains("either text or column"));
        assert!(err(table, "{kind: line_chart, series: s, column: b}", data).contains("one of entity, series or column"));
    }

    #[test]
    fn bullet_charts_take_a_value_from_three_places() {
        let page = |tile: &str, data: &str| format!("target: t\nlayouts: {{l: {{a: {{x: 0, y: 0, width: 64, height: 20}}}}}}\npages: {{p: {{layout: l, tiles: {{a: {tile}}}, data: {data}}}}}\n");
        let m = load(&page("{kind: bullet_chart, entity: sensor.a, target: 80, ranges: [50, 75]}", "{sensors: {sensor.a: {state: '62'}}}")).unwrap();
        assert_eq!(m.sensor_entities(), ["sensor.a"], "an entity's state, not its history");
        assert!(m.series().is_empty());
        assert!(matches!(m.pages[0].layers[0].tile, TileSpec::BulletChart { min: 0.0, max: 100.0, target: Some(80.0), .. }));
        let m = load(&page("{kind: bullet_chart, series: load}", "{series: {load: [1, 2]}}")).unwrap();
        assert_eq!(m.pushed(), ["load"]);
        let err = |tile: &str, data: &str| format!("{:#}", load(&page(tile, data)).unwrap_err());
        assert!(err("{kind: bullet_chart, entity: sensor.a}", "{}").contains("bullet_chart tile needs sources.home_assistant"));
        assert!(err("{kind: bullet_chart, series: load}", "{}").contains("needs sources.http"));
        assert!(err("{kind: bullet_chart}", "{}").contains("one of entity, series or column"));
        assert!(err("{kind: bullet_chart, series: a, column: b}", "{}").contains("one of entity, series or column"));
        assert!(err("{kind: bullet_chart, series: a, min: 5, max: 5}", "{}").contains("max must be above its min"));
        assert!(err("{kind: bullet_chart, series: a, ranges: [80, 60]}", "{}").contains("ranges"));
        assert!(err("{kind: bullet_chart, series: a, ranges: [60, 100]}", "{}").contains("ranges"));
        assert!(err("{kind: bullet_chart, series: a, target: 1, target_column: t}", "{}").contains("either target or target_column"));
        assert!(err("{kind: bullet_chart, series: a, target_column: t}", "{series: {a: [1]}}").contains("column is for a tile on a table's repeat row"));
        let table = "{kind: table, data: kpis, rows: [{height: 6, repeat: true, tiles: [{x: 0, width: 60, tile: {kind: bullet_chart, column: v, target_column: t}}]}]}";
        let m = load(&page(table, "{tables: {kpis: [{v: 5, t: 9}]}}")).unwrap();
        let columns: Vec<(String, &str)> = m.table_columns()["kpis"].clone().into_iter().collect();
        assert_eq!(columns, [("t".to_string(), "number"), ("v".to_string(), "number")]);
    }

    #[test]
    fn commands_feed_tables_and_series() {
        use crate::command::Feed;
        let yaml = |commands: &str| {
            format!(
                "target: t\nsecrets: command-secrets.yaml\nsources: {{commands: {commands}}}\n\
                 layouts: {{l: {{a: {{x: 0, y: 0, width: 60, height: 20}}, b: {{x: 0, y: 30, width: 60, height: 20}}}}}}\n\
                 tiles: {{sheet: {{kind: table, data: rows, rows: [{{height: 6, repeat: true, tiles: [{{x: 0, width: 60, tile: {{kind: text, column: n}}}}]}}]}}}}\n\
                 pages: {{p: {{layout: l, tiles: {{a: sheet, b: {{kind: bar_chart, series: orders}}}}}}}}\n"
            )
        };
        let dir = std::env::temp_dir().join(format!("panel-ddp-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("command-secrets.yaml"), "ha_token: abc123\n").unwrap();
        let both = "[{table: rows, run: [sqlite3, -json, x.db, 'select 1'], every: 5, env: {PGPASSWORD: {secret: ha_token}}}, \
                    {series: orders, column: n, run: [sh, -c, 'echo 1']}]";
        let m = load(&yaml(both)).unwrap();
        assert!(m.http.is_none(), "the names are fed without the endpoint");
        let c = &m.commands[0];
        assert_eq!((&c.feed, c.every, c.timeout), (&Feed::Table("rows".into()), 5, 30));
        assert_eq!(c.run, ["sqlite3", "-json", "x.db", "select 1"]);
        assert_eq!(c.env, [("PGPASSWORD".to_string(), "abc123".to_string())]);
        assert_eq!(c.dir, dir, "it runs beside the config");
        assert_eq!((&m.commands[1].feed, m.commands[1].every), (&Feed::Series("orders".into()), 60));
        let err = |commands: &str| format!("{:#}", load(&yaml(commands)).unwrap_err());
        assert!(err("[{table: rows, run: [a]}]").contains("a bar_chart tile needs sources.http or a command"));
        assert!(err("[{series: orders, run: [a]}]").contains("needs sources.http or a command"), "the table is not fed");
        let rest = "{table: rows, run: [a]}, {series: orders, run: [a]}";
        assert!(err(&format!("[{rest}, {{series: other, run: [a]}}]")).contains("nothing draws the series other"));
        assert!(err(&format!("[{rest}, {{table: other, run: [a]}}]")).contains("nothing draws the table other"));
        assert!(err(&format!("[{rest}, {{run: [a]}}]")).contains("entry 3: takes either table or series"));
        assert!(err(&format!("[{rest}, {{table: rows, series: orders, run: [a]}}]")).contains("takes either table or series"));
        assert!(err(&format!("[{rest}, {{table: rows, run: []}}]")).contains("run needs a program"));
        assert!(err(&format!("[{rest}, {{table: rows, run: [a], every: 0}}]")).contains("at least 1"));
        assert!(err(&format!("[{rest}, {{table: rows, run: [a], column: n}}]")).contains("column is for a series"));
        assert!(err(&format!("[{rest}, {{table: rows, run: [a], env: {{X: {{secret: missing}}}}}}]")).contains("env X"));
        assert!(err(&format!("[{rest}, {{table: rows, run: a b}}]")).contains("sources"), "run is a list");
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
        let http = |token: &str| {
            let yaml = BASE.replace("sources:\n", &format!("secrets: test-secrets.yaml\nsources:\n  http: {{{token}}}\n"));
            load(&yaml).map(|m| m.http.unwrap().token)
        };
        assert_eq!(http("").unwrap(), None, "the token is optional");
        assert_eq!(http("token: {secret: ha_token}").unwrap().as_deref(), Some("abc123"));
        assert!(http("token: {secret: missing}").is_err());
        assert!(http("token: abc").is_err(), "no literal tokens");
    }

    #[test]
    fn the_committed_configs_load_and_follow_the_schema() {
        let v = validator();
        for name in ["demo.yaml", "dashboard.example.yaml"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
            super::load(&path).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            let text = std::fs::read_to_string(&path).unwrap();
            let errors: Vec<String> = v.iter_errors(&as_json(&text)).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
    }

    #[test]
    fn the_panel_size_is_configurable() {
        let wide = BASE.replace("target: wled.local\n", "target: wled.local\nwidth: 128\nheight: 64\n");
        let m = load(&wide.replace("width: 64, height: 64", "width: 128, height: 64")).unwrap();
        assert_eq!((m.width, m.height), (128, 64));
        assert_eq!(m.alert_area.top_left, embedded_graphics::prelude::Point::new(53, 21), "centred on the panel");
        assert_eq!(load(BASE).unwrap().alert_area.top_left, embedded_graphics::prelude::Point::new(21, 21));
        let err = format!("{:#}", load(&wide.replace("width: 64, height: 64", "width: 129, height: 64")).unwrap_err());
        assert!(err.contains("must fit the 128x64 panel"), "{err}");
        assert!(load(&BASE.replace("target: wled.local\n", "target: wled.local\nwidth: 0\n")).is_err());
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
