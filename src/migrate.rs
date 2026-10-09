//! `panel-ddp migrate`: a TOML or JSON configuration rewritten as a YAML
//! one, its Home Assistant token moved to the secrets file. The regions
//! become a layout named `classic` (with a `background` region under them
//! when a page has a background), each distinct tile is named once, and
//! settings left at their defaults are left out. `same_drawing` checks the
//! result against the original.

// Older configs were for a 64x64 panel only.
use crate::canvas::{DEFAULT_HEIGHT as HEIGHT, DEFAULT_WIDTH as WIDTH};
use crate::config::{ArtCacheConfig, PageData, TileSpec, Units};
use crate::legacy::{Background, Config, Regions, Slot, Tiles};
use crate::model::Model;
use crate::palette::Overrides;
use anyhow::{bail, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The secret the token is moved to.
pub const TOKEN_SECRET: &str = "home_assistant_token";

pub struct Migrated {
    pub yaml: String,
    /// The Home Assistant token, for the secrets file.
    pub token: Option<String>,
}

/// Migrates `path` for a new file in `out_dir`, its paths made relative
/// to that folder.
pub fn migrate(path: &Path, out_dir: &Path) -> Result<Migrated> {
    let cfg = Config::read(path)?;
    let rel = |p: &Path| relative(p, out_dir);
    let mut root = Map::new();
    root.insert("target".into(), json!(cfg.target));
    if cfg.fps != 10 {
        root.insert("fps".into(), json!(cfg.fps));
    }
    if cfg.gamma != 2.2 {
        root.insert("gamma".into(), json!(cfg.gamma));
    }
    if cfg.temperature != Units::default() {
        root.insert("temperature".into(), json!(cfg.temperature));
    }
    if rel(&cfg.gaps) != Path::new("2d-gaps.json") {
        root.insert("gaps".into(), json!(rel(&cfg.gaps)));
    }

    let model = cfg_model(path)?;
    let mut sources = Map::new();
    if let Some(w) = &cfg.weather {
        let mut w = w.clone();
        if w.units == Some(cfg.temperature) {
            w.units = None;
        }
        sources.insert("weather".into(), minimal::<crate::config::WeatherConfig>(json!(w)));
    }
    let mut token = None;
    if let Some(h) = &cfg.home_assistant {
        token = Some(h.token.clone());
        let mut v = json!({"url": h.url, "token": {"secret": TOKEN_SECRET}});
        if let Some(p) = &h.media_player {
            v["media_player"] = json!(p);
        }
        if h.refresh_seconds != 10 {
            v["refresh_seconds"] = json!(h.refresh_seconds);
        }
        sources.insert("home_assistant".into(), v);
    }
    if let Some(sp) = &cfg.spotify {
        let mut sp = sp.clone();
        sp.token_file = rel(&sp.token_file);
        sources.insert("spotify".into(), minimal::<crate::config::SpotifyConfig>(json!(sp)));
    }
    if model.wants_pictures() {
        let mut v = json!({"dir": rel(&cfg.frame.dir), "seconds": cfg.frame.seconds, "shuffle": cfg.frame.shuffle});
        let defaults = json!({"dir": "frame", "seconds": 10.0, "shuffle": false});
        v.as_object_mut().unwrap().retain(|k, val| defaults[k.as_str()] != *val);
        sources.insert("pictures".into(), v);
    }
    if !sources.is_empty() {
        root.insert("sources".into(), Value::Object(sources));
    }
    let mut cache = cfg.art_cache.clone();
    cache.dir = rel(&cache.dir);
    let cache = minimal::<ArtCacheConfig>(json!(cache));
    if cache.as_object().is_some_and(|m| !m.is_empty()) {
        root.insert("art_cache".into(), cache);
    }
    if let Some(f) = &cfg.art_file {
        root.insert("art_file".into(), json!(rel(f)));
    }
    if cfg.art_open {
        root.insert("art_open".into(), json!(true));
    }
    if let Some(colors) = non_empty(&cfg.colors) {
        root.insert("schemes".into(), json!({"default": colors}));
    }

    // The pages as legacy files have them: [tiles] alone, or pages.
    let plain = cfg.pages.is_empty();
    let pages: Vec<(String, Option<f32>, Tiles, PageData)> = if plain {
        vec![("dashboard".into(), None, cfg.tiles.clone(), PageData::default())]
    } else {
        cfg.pages.iter().enumerate().map(|(i, p)| (format!("page-{}", i + 1), Some(p.seconds), p.tiles(), p.data.clone())).collect()
    };
    // Album art in a background with nothing to feed it never draws; the
    // new format refuses it, so it goes.
    let media = cfg.spotify.is_some() || cfg.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some());
    let background = |t: &Tiles, d: &PageData| match background_tile(t) {
        Some(TileSpec::Art { .. }) if !media && d.cover.is_none() => None,
        b => b,
    };
    let with_background = pages.iter().any(|(_, _, t, d)| background(t, d).is_some());
    root.insert("layouts".into(), json!({"classic": layout(&cfg.regions, with_background)}));

    // Tiles, each distinct one named once.
    let mut named: Vec<(String, Value)> = Vec::new();
    let mut name_of = |v: Value, base: &str| -> String {
        if let Some((n, _)) = named.iter().find(|(_, t)| *t == v) {
            return n.clone();
        }
        let mut name = base.to_string();
        let mut i = 2;
        while named.iter().any(|(n, _)| *n == name) {
            name = format!("{base}-{i}");
            i += 1;
        }
        named.push((name.clone(), v));
        name
    };
    let mut page_values = Map::new();
    let page_seconds = most_common(pages.iter().filter_map(|(_, s, _, _)| *s)).unwrap_or(10.0);
    for (name, seconds, tiles, data) in &pages {
        let mut placed = Map::new();
        if let Some(t) = background(tiles, data) {
            let base = if matches!(t, TileSpec::Picture { .. }) { "pictures" } else { "cover-background" };
            placed.insert("background".into(), json!(name_of(tile_value(&t, &Overrides::default()), base)));
        }
        for slot in Slot::ALL {
            let (spec, colors) = match tiles.tile(slot) {
                Some(e) => (e.spec.clone(), e.colors),
                None => (tiles.hub.spec.to_tile(), tiles.hub.colors),
            };
            if matches!(spec, TileSpec::Blank) {
                continue;
            }
            let base = tile_base_name(&spec, slot);
            placed.insert(slot.name().into(), json!(name_of(tile_value(&spec, &colors), &base)));
        }
        let mut p = Map::new();
        p.insert("layout".into(), json!("classic"));
        if let Some(s) = seconds.filter(|s| *s != page_seconds) {
            p.insert("seconds".into(), json!(s));
        }
        p.insert("tiles".into(), Value::Object(placed));
        let mut data = strip_nulls(minimal::<PageData>(json!(data)));
        if let Some(w) = data.get_mut("weather") {
            *w = minimal::<crate::config::WeatherData>(w.take());
        }
        if let Some(Value::Object(sensors)) = data.get_mut("sensors") {
            for s in sensors.values_mut() {
                *s = minimal::<crate::config::SensorData>(s.take());
            }
        }
        if data.as_object().is_some_and(|m| !m.is_empty()) {
            p.insert("data".into(), data);
        }
        page_values.insert(name.clone(), Value::Object(p));
    }
    root.insert("tiles".into(), Value::Object(named.into_iter().collect()));
    if page_seconds != 10.0 {
        root.insert("page_seconds".into(), json!(page_seconds));
    }
    root.insert("pages".into(), Value::Object(page_values));
    if !cfg.alerts.is_empty() {
        let alerts: Vec<Value> = cfg.alerts.iter().map(|a| minimal::<crate::config::Alert>(json!(a))).collect();
        root.insert("alerts".into(), json!(alerts));
    }
    let hub = cfg.regions.hub;
    if (hub.x, hub.y, hub.width, hub.height) != (21, 21, 22, 22) {
        root.insert("alert_area".into(), json!({"x": hub.x, "y": hub.y, "width": hub.width, "height": hub.height}));
    }

    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut yaml = format!("# yaml-language-server: $schema=panel-ddp.schema.json\n# Migrated from {name} by panel-ddp migrate.\n");
    emit_top(&Value::Object(strip_nulls_map(root)), &mut yaml);
    Ok(Migrated { yaml, token })
}

/// `p` from `base`, both taken from the current folder when relative:
/// "art-cache" beside it, "../art-cache" one folder up.
fn relative(p: &Path, base: &Path) -> PathBuf {
    let (to, from) = (parts(p), parts(base));
    let common = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to[common..] {
        out.push(c);
    }
    out
}

/// The folders from the root down to `p`, taken from the current folder
/// when relative, with "." and ".." worked out.
fn parts(p: &Path) -> Vec<std::ffi::OsString> {
    use std::path::Component;
    let full = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    let mut v = Vec::new();
    for c in full.components() {
        match c {
            Component::Normal(s) => v.push(s.to_os_string()),
            Component::ParentDir => {
                v.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    v
}

/// `p` absolute, for comparing where two paths lead.
fn resolved(p: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    out.extend(parts(p));
    out
}

fn cfg_model(path: &Path) -> Result<Model> {
    crate::legacy::load(path)
}

/// The five regions, and a background region under them all.
fn layout(r: &Regions, with_background: bool) -> Value {
    let mut m = Map::new();
    if with_background {
        let lowest = Slot::ALL.iter().map(|&s| r.get(s).z).min().unwrap_or(0).min(0);
        m.insert("background".into(), json!({"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT, "z": lowest - 1}));
    }
    for slot in Slot::ALL {
        let g = r.get(slot);
        let mut v = json!({"x": g.x, "y": g.y, "width": g.width, "height": g.height});
        if g.z != 0 {
            v["z"] = json!(g.z);
        }
        m.insert(slot.name().into(), v);
    }
    Value::Object(m)
}

fn background_tile(t: &Tiles) -> Option<TileSpec> {
    use crate::config::{default_corner_alpha, ArtShape, Idle};
    match t.background.clone()? {
        Background::Media { alpha } => Some(TileSpec::Art {
            shape: ArtShape::Square,
            spin: false,
            paused_alpha: 1.0,
            corner_alpha: default_corner_alpha(),
            alpha,
            idle: Idle::None,
        }),
        Background::Frame { alpha } => Some(TileSpec::Picture { alpha }),
        Background::None => None,
    }
}

fn tile_value(spec: &TileSpec, colors: &Overrides) -> Value {
    let mut v = minimal::<TileSpec>(json!(spec));
    if let Some(c) = non_empty(colors) {
        v["colors"] = c;
    }
    v
}

/// A name for a tile: its label for sensors and progress bars, else its
/// kind, the hub's art being the cover.
fn tile_base_name(spec: &TileSpec, slot: Slot) -> String {
    let slug = |s: &str| {
        let s: String = s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        s.trim_matches('-').to_string()
    };
    match spec {
        TileSpec::Sensor { label, .. } | TileSpec::Progress { label, .. } if !slug(label).is_empty() => slug(label),
        TileSpec::Art { .. } if slot == Slot::Hub => "cover".into(),
        TileSpec::Clock { .. } => "clock".into(),
        TileSpec::Date => "date".into(),
        TileSpec::Text { .. } => "text".into(),
        TileSpec::Weather => "weather".into(),
        TileSpec::NowPlaying => "now-playing".into(),
        TileSpec::Art { .. } => "art".into(),
        TileSpec::Picture { .. } => "pictures".into(),
        TileSpec::Sensor { .. } => "sensor".into(),
        TileSpec::Sparkline { .. } => "sparkline".into(),
        TileSpec::Progress { .. } => "progress".into(),
        TileSpec::Blank => "blank".into(),
    }
}

fn non_empty(o: &Overrides) -> Option<Value> {
    let v = json!(o);
    v.as_object().is_some_and(|m| !m.is_empty()).then_some(v)
}

fn most_common(v: impl Iterator<Item = f32>) -> Option<f32> {
    let mut counts: Vec<(f32, usize)> = Vec::new();
    for s in v {
        match counts.iter_mut().find(|(x, _)| *x == s) {
            Some((_, n)) => *n += 1,
            None => counts.push((s, 1)),
        }
    }
    counts.into_iter().max_by_key(|&(_, n)| n).map(|(s, _)| s)
}

/// `v` without the keys that read back the same when left out: those at
/// their defaults.
fn minimal<T: DeserializeOwned + std::fmt::Debug>(v: Value) -> Value {
    let v = strip_nulls(v);
    let Value::Object(mut m) = v else { return v };
    let want = match serde_json::from_value::<T>(Value::Object(m.clone())) {
        Ok(t) => format!("{t:?}"),
        Err(_) => return Value::Object(m),
    };
    let keys: Vec<String> = m.keys().cloned().collect();
    for k in keys.iter().filter(|k| *k != "kind") {
        let mut without = m.clone();
        without.retain(|key, _| key != k);
        if serde_json::from_value::<T>(Value::Object(without.clone())).is_ok_and(|t| format!("{t:?}") == want) {
            m = without;
        }
    }
    Value::Object(m)
}

fn strip_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(strip_nulls_map(m)),
        Value::Array(a) => Value::Array(a.into_iter().map(strip_nulls).collect()),
        v => v,
    }
}

fn strip_nulls_map(m: Map<String, Value>) -> Map<String, Value> {
    m.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k, strip_nulls(v))).collect()
}

/// A page's layers that can draw something: not blank, and not album art
/// with no source and no cover of the page's own.
fn drawn(m: &Model, p: &crate::model::Page) -> String {
    let media = m.spotify.is_some() || m.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some());
    let layers: Vec<_> = p
        .layers
        .iter()
        .filter(|l| match l.tile {
            TileSpec::Blank => false,
            TileSpec::Art { idle, .. } => media || p.data.cover.is_some() || idle != crate::config::Idle::None,
            _ => true,
        })
        .collect();
    format!("{layers:?}")
}

/// Checks that the migrated model draws what the original does: every
/// page's layers and data, their times, the alerts and what the sources
/// are told. Blank layers, and art that cannot show, do not count.
pub fn same_drawing(old: &Model, new: &Model) -> Result<()> {
    if old.pages.len() != new.pages.len() {
        bail!("{} pages became {}", old.pages.len(), new.pages.len());
    }
    for (i, (a, b)) in old.pages.iter().zip(&new.pages).enumerate() {
        if drawn(old, a) != drawn(new, b) {
            bail!("page {} ({}) draws differently", i + 1, b.name);
        }
        if format!("{:?}", a.data) != format!("{:?}", b.data) {
            bail!("page {} ({}) has other data", i + 1, b.name);
        }
        if a.seconds.is_some() && a.seconds != b.seconds {
            bail!("page {} ({}) shows for {:?} s, not {:?}", i + 1, b.name, b.seconds, a.seconds);
        }
    }
    let same = |what: &str, x: String, y: String| if x == y { Ok(()) } else { bail!("{what} differ") };
    // Paths compare by where they lead.
    let paths = |m: &Model| {
        let mut m = m.clone();
        m.art_cache.dir = resolved(&m.art_cache.dir);
        m.frame.dir = resolved(&m.frame.dir);
        m.art_file = m.art_file.as_deref().map(resolved);
        if let Some(sp) = m.spotify.as_mut() {
            sp.token_file = resolved(&sp.token_file);
        }
        m
    };
    let (old, new) = (&paths(old), &paths(new));
    same("the alerts", format!("{:?}", old.alerts), format!("{:?}", new.alerts))?;
    same("the alert areas", format!("{:?}", old.alert_area), format!("{:?}", new.alert_area))?;
    same("fps, gamma or temperature", format!("{:?}", (old.fps, old.gamma, old.temperature)), format!("{:?}", (new.fps, new.gamma, new.temperature)))?;
    same("the weather sources", format!("{:?}", old.weather), format!("{:?}", new.weather))?;
    same("the Home Assistant sources", format!("{:?}", old.home_assistant), format!("{:?}", new.home_assistant))?;
    same("the Spotify sources", format!("{:?}", old.spotify), format!("{:?}", new.spotify))?;
    same("the art caches", format!("{:?}", old.art_cache), format!("{:?}", new.art_cache))?;
    same("the pictures", format!("{:?}", old.frame), format!("{:?}", new.frame))?;
    same("the art files", format!("{:?}", (&old.art_file, old.art_open)), format!("{:?}", (&new.art_file, new.art_open)))?;
    same("the targets", old.target.clone(), new.target.clone())
}

// A small YAML writer: block style, with maps and lists that fit on a line
// written inline, as a person would.

const LINE: usize = 96;

fn emit_top(v: &Value, out: &mut String) {
    let Value::Object(m) = v else { return };
    for (k, val) in m {
        out.push('\n');
        emit_entry(k, val, 0, out);
    }
}

fn emit_entry(k: &str, v: &Value, indent: usize, out: &mut String) {
    let head = format!("{}{}:", " ".repeat(indent), key(k));
    let inline = flow(v);
    if is_scalar(v) || head.len() + 1 + inline.len() <= LINE {
        out.push_str(&format!("{head} {inline}\n"));
    } else {
        out.push_str(&head);
        out.push('\n');
        emit_block(v, indent + 2, out);
    }
}

fn emit_block(v: &Value, indent: usize, out: &mut String) {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                emit_entry(k, val, indent, out);
            }
        }
        Value::Array(a) => {
            for item in a {
                let dash = format!("{}- ", " ".repeat(indent));
                let inline = flow(item);
                if dash.len() + inline.len() <= LINE || !matches!(item, Value::Object(_)) {
                    out.push_str(&format!("{dash}{inline}\n"));
                } else {
                    // The first entry on the dash's line, the rest under it.
                    let mut body = String::new();
                    emit_block(item, indent + 2, &mut body);
                    out.push_str(&dash);
                    out.push_str(body.trim_start());
                }
            }
        }
        v => out.push_str(&format!("{}{}\n", " ".repeat(indent), flow(v))),
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

fn flow(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number(n),
        Value::String(s) => string(s),
        Value::Array(a) => format!("[{}]", a.iter().map(flow).collect::<Vec<_>>().join(", ")),
        Value::Object(m) => {
            format!("{{{}}}", m.iter().map(|(k, v)| format!("{}: {}", key(k), flow(v))).collect::<Vec<_>>().join(", "))
        }
    }
}

/// Numbers as written: a float that came from an `f32` in its shortest
/// form, 0.12 and not 0.11999999731779099.
fn number(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if n.is_f64() && f == (f as f32) as f64 => {
            let s = (f as f32).to_string();
            if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
        }
        _ => n.to_string(),
    }
}

fn key(k: &str) -> String {
    string(k)
}

/// A string plain when YAML reads it back as that string, else quoted.
fn string(s: &str) -> String {
    // Words some YAML readers take for booleans or null; a lone y or n
    // only very old ones, so those stay plain.
    const WORDS: [&str; 19] = [
        "true", "false", "yes", "no", "on", "off", "null", "True", "False", "Yes", "No", "On", "Off", "Null", "NULL",
        "TRUE", "FALSE", "YES", "NO",
    ];
    let plain = !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '/')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || " _-./".contains(c))
        && !s.ends_with(' ')
        && !WORDS.contains(&s);
    if plain { s.to_string() } else { serde_json::to_string(s).expect("a string serialises") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_writer_reads_back_the_same() {
        let v = json!({
            "target": "wled.local",
            "on": "on",
            "colour": "#ff0000",
            "alpha": 0.12f32,
            "latitude": 40.712776,
            "list": [1, "two", {"a": "b: c"}],
            "nested": {"deep": {"x": [], "y": {}, "z": "a long string that will not fit on one line with the rest of the map around it at all"}},
            "area": {"x": 1, "y": 2, "yes": "no"},
            "empty": ""
        });
        let mut out = String::new();
        emit_top(&v, &mut out);
        assert!(out.contains("alpha: 0.12\n"), "{out}");
        let back: Value = serde_yaml_ng::from_str(&out).unwrap();
        assert_eq!(back["alpha"].as_f64().map(|f| f as f32), Some(0.12));
        let mut expect = v.clone();
        expect["alpha"] = back["alpha"].clone();
        assert_eq!(back, expect, "{out}");
    }

    #[test]
    fn a_migrated_file_draws_the_same() {
        let dir = std::env::temp_dir().join(format!("panel-ddp-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old.toml");
        std::fs::write(
            &old,
            r##"
target = "wled.local"
fps = 12
[colors]
text = "#fafafa"
[spotify]
client_id = "abc"
[home_assistant]
url = "http://ha"
token = "secret-token"
[regions]
hub = { x = 16, y = 16, width = 32, height = 32, z = -2 }
[[alerts]]
entity = "binary_sensor.leak"
label = "LEAK"
[[pages]]
seconds = 3
top_left = { kind = "clock", colors = { accent = "#ff000080" } }
hub = { kind = "media", shape = "faded" }
[[pages]]
seconds = 4
background = { kind = "media", alpha = 0.3 }
bottom_left = { kind = "progress", entity = "sensor.p", label = "Print" }
data = { sensors = { "sensor.p" = { sweep = [0, 100] } } }
"##,
        )
        .unwrap();
        let m = migrate(&old, &dir).unwrap();
        assert_eq!(m.token.as_deref(), Some("secret-token"));
        assert!(!m.yaml.contains("secret-token"), "the token stays out of the file");
        let new = dir.join("new.yaml");
        std::fs::write(&new, &m.yaml).unwrap();
        std::fs::write(dir.join("secrets.yaml"), format!("{TOKEN_SECRET}: secret-token\n")).unwrap();
        let (a, b) = (crate::legacy::load(&old).unwrap(), crate::format::load(&new).unwrap());
        same_drawing(&a, &b).unwrap();
        assert!(m.yaml.contains("print: {kind: progress"), "{}", m.yaml);
    }

    #[test]
    fn paths_are_relative_to_the_new_file() {
        assert_eq!(relative(Path::new("/a/b/art-cache"), Path::new("/a/b")), Path::new("art-cache"));
        assert_eq!(relative(Path::new("/a/b/art-cache"), Path::new("/a/c")), Path::new("../b/art-cache"));
        assert_eq!(relative(Path::new("/a/b/./x/../art-cache"), Path::new("/a/b/")), Path::new("art-cache"));
    }

    #[test]
    fn defaults_are_left_out() {
        let clock = minimal::<TileSpec>(json!(TileSpec::Clock { seconds: Default::default(), dot_size: 2 }));
        assert_eq!(clock, json!({"kind": "clock"}));
    }
}
