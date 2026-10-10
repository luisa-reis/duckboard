//! What the tiles draw from. A `Snapshot` is the latest of everything the
//! sources have fetched; the frame loop takes a copy each frame. The sources
//! run on their own threads and each refreshes its part on its own clock,
//! so a slow or dead service never stalls a frame.

use crate::artcache::ArtCache;
use crate::model::Model;
use crate::picture::Scaled;
use crate::ha::{self, Art, Media, Sensor};
use crate::http;
use crate::spotify;
use crate::weather::{self, Weather};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How often the charts' histories are fetched again.
const HISTORY_REFRESH: Duration = Duration::from_secs(60);

/// A value in a row pushed for a table: a line of text, or the numbers of
/// a chart.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Series(Vec<f64>),
}

/// A row pushed for a table: its values by column.
pub type Record = HashMap<String, Value>;

/// The most rows a table's data keeps.
pub const MAX_ROWS: usize = 256;

/// The rows in pushed JSON: an array of objects, or one object for a
/// single row. A string, a number or a boolean is text as written, a null
/// is empty text, and an array of numbers is a chart's values.
pub fn records(json: &serde_json::Value) -> Result<Vec<Record>, String> {
    use serde_json::Value as Json;
    let rows = match json {
        Json::Array(rows) => rows.iter().collect(),
        Json::Object(_) => vec![json],
        _ => return Err("the rows are a JSON array of objects, column to value".into()),
    };
    let mut records = Vec::with_capacity(rows.len());
    for (i, row) in rows.into_iter().enumerate() {
        let Json::Object(row) = row else { return Err(format!("row {}: not an object, column to value", i + 1)) };
        let mut record = Record::new();
        for (column, value) in row {
            let value = match value {
                Json::String(s) => Value::Text(s.clone()),
                Json::Number(n) => Value::Text(n.to_string()),
                Json::Bool(b) => Value::Text(b.to_string()),
                Json::Null => Value::Text(String::new()),
                Json::Array(a) => match a.iter().map(Json::as_f64).collect::<Option<Vec<f64>>>() {
                    Some(numbers) => Value::Series(numbers),
                    None => return Err(format!("row {}, column {column}: an array must be of numbers", i + 1)),
                },
                Json::Object(_) => return Err(format!("row {}, column {column}: not text, a number or an array", i + 1)),
            };
            record.insert(column.clone(), value);
        }
        records.push(record);
    }
    Ok(records)
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub weather: Option<Weather>,
    /// By entity id.
    pub sensors: HashMap<String, Sensor>,
    pub media: Option<Media>,
    /// An entity's history over some hours, oldest first, for the
    /// charts; shared, since the snapshot is copied every frame.
    pub series: HashMap<(String, u32), Arc<Vec<f64>>>,
    /// The series pushed over HTTP, by name, oldest value first.
    pub pushed: HashMap<String, Arc<Vec<f64>>>,
    /// The rows pushed over HTTP for the tables, by data name.
    pub tables: HashMap<String, Arc<Vec<Record>>>,
}

impl Snapshot {
    /// Made-up data for previews and demos that must not touch the network:
    /// every sensor the config names sweeps 0..100 over `SWEEP_SECONDS` and
    /// holds full for `HOLD_SECONDS`, every chart draws the same two
    /// waves, every table row is filled with them, with `--` and with 62,
    /// alerts raise for five seconds of every
    /// thirty, and a gradient plays as album art. Timed in seconds, so it
    /// runs the same at any frame rate.
    pub fn sample(cfg: &Model, frame: u32) -> Self {
        const SWEEP_SECONDS: u32 = 15;
        const HOLD_SECONDS: u32 = 3;
        let fps = cfg.fps.max(1);
        let (sweep, hold) = (SWEEP_SECONDS * fps, HOLD_SECONDS * fps);
        let t = frame % (sweep + hold);
        let value = (t.min(sweep) as f64 * 100.0 / sweep as f64 * 10.0).round() / 10.0;
        let s = frame % (30 * fps);
        let alert_on = s >= 5 * fps && s < 10 * fps;
        let sensors: HashMap<String, Sensor> = cfg
            .sensor_entities()
            .into_iter()
            .map(|e| (e, Sensor { state: format!("{value}"), unit: Some("%".into()) }))
            .collect();
        let gradient = |w: u32, h: u32| {
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                for x in 0..w {
                    rgb.extend_from_slice(&[(x * 240 / w) as u8, (y * 240 / h) as u8, 180 - (x * 90 / w) as u8]);
                }
            }
            rgb
        };
        let waves: Arc<Vec<f64>> =
            Arc::new((0..96).map(|i| (i as f64 / 7.0).sin() * 3.0 + (i as f64 / 2.3).sin() + i as f64 / 24.0).collect());
        let series = cfg.series().into_iter().map(|key| (key, Arc::clone(&waves))).collect();
        let pushed = cfg.pushed().into_iter().map(|name| (name, Arc::clone(&waves))).collect();
        // A row for every place a table has one, with a value for each
        // column its tiles read.
        let mut rows: HashMap<String, Vec<Record>> = HashMap::new();
        for layer in cfg.pages.iter().flat_map(|p| &p.layers) {
            let Some((table, row)) = &layer.record else { continue };
            let rows = rows.entry(table.clone()).or_default();
            rows.resize(rows.len().max(row + 1), Record::new());
            for (column, holds) in layer.tile.columns() {
                let value = match holds {
                    "numbers" => Value::Series(waves.to_vec()),
                    "number" => Value::Text("62".into()),
                    _ => Value::Text("--".into()),
                };
                rows[*row].insert(column.clone(), value);
            }
        }
        let tables = rows.into_iter().map(|(name, rows)| (name, Arc::new(rows))).collect();
        let scaled = Scaled::from_fn(&cfg.art_sizes(), gradient);
        let mut snap = Self {
            weather: Some(Weather { temperature: 21.4, code: 61, is_day: true }),
            sensors,
            series,
            pushed,
            tables,
            media: Some(Media {
                playing: true,
                title: "Sample Song Title".into(),
                artist: "Sample Artist".into(),
                art: Some(Art { url: String::new(), scaled, original: None }),
            }),
        };
        snap.set_alerts(cfg, alert_on);
        snap
    }

    /// Raises every alert the config names, or lowers them all.
    pub fn set_alerts(&mut self, cfg: &Model, on: bool) {
        for a in &cfg.alerts {
            let state = if on { a.state.clone() } else { format!("not {}", a.state) };
            self.sensors.insert(a.entity.clone(), Sensor { state, unit: None });
        }
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

/// Starts one thread per configured source. Each keeps the last good value
/// and logs a failure only when its message changes, so a service that is
/// down does not fill the log at every retry.
/// The running sources; dropping it stops them, each within a second.
#[must_use = "the sources stop when this is dropped"]
pub struct Sources {
    stop: Arc<AtomicBool>,
}

impl Drop for Sources {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Sleeps for `d`, a second at a time; false once `stop` is set.
fn nap(stop: &AtomicBool, d: Duration) -> bool {
    let mut left = d;
    while !left.is_zero() {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        let step = left.min(Duration::from_secs(1));
        thread::sleep(step);
        left -= step;
    }
    !stop.load(Ordering::SeqCst)
}

pub fn spawn_sources(cfg: &Model, shared: &Shared) -> Sources {
    let stop = Arc::new(AtomicBool::new(false));
    let cache = ArtCache::for_model(cfg);
    let gamma = cfg.gamma;
    if let Some(w) = cfg.weather.clone() {
        let shared = Arc::clone(shared);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let agent = ureq::AgentBuilder::new().build();
            let mut last_err = None;
            loop {
                match weather::fetch(&agent, &w) {
                    Ok(v) => {
                        shared.lock().unwrap().weather = Some(v);
                        last_err = None;
                    }
                    Err(e) => log_changed(&mut last_err, "weather", e),
                }
                if !nap(&stop, Duration::from_secs(60 * w.refresh_minutes.max(1))) {
                    break;
                }
            }
        });
    }

    if let Some(sp) = cfg.spotify.clone() {
        let shared = Arc::clone(shared);
        let cache = cache.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut last_err = None;
            let mut client = None;
            loop {
                if client.is_none() {
                    match spotify::Client::load(&sp, cache.clone(), gamma) {
                        Ok(c) => client = Some(c),
                        Err(e) => log_changed(&mut last_err, "spotify", e),
                    }
                }
                if let Some(c) = client.as_mut() {
                    let previous = shared.lock().unwrap().media.clone();
                    match c.currently_playing(previous.as_ref()) {
                        Ok(v) => {
                            shared.lock().unwrap().media = v;
                            last_err = None;
                        }
                        Err(e) => log_changed(&mut last_err, "spotify", e),
                    }
                }
                if !nap(&stop, Duration::from_secs(sp.refresh_seconds.max(1))) {
                    break;
                }
            }
        });
    }

    if let Some(h) = cfg.home_assistant.clone() {
        let shared = Arc::clone(shared);
        let entities = cfg.sensor_entities();
        let series = cfg.series();
        // With Spotify configured, the media player here is left alone.
        let player = if cfg.spotify.is_some() { None } else { h.media_player.clone() };
        let cache = cache.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let client = ha::Client::new(&h, cache, gamma);
            // One error slot per request, so a missing entity is reported
            // once and a working one next to it does not reset that.
            let mut errors = vec![None; entities.len() + 1];
            let mut series_errors = vec![None; series.len()];
            let mut histories_at: Option<Instant> = None;
            loop {
                for (entity, last_err) in entities.iter().zip(errors.iter_mut()) {
                    match client.sensor(entity) {
                        Ok(v) => {
                            shared.lock().unwrap().sensors.insert(entity.clone(), v);
                            *last_err = None;
                        }
                        Err(e) => log_changed(last_err, "home assistant", e),
                    }
                }
                // The histories move slowly: once every `HISTORY_REFRESH`.
                if histories_at.is_none_or(|at| at.elapsed() >= HISTORY_REFRESH) {
                    histories_at = Some(Instant::now());
                    for (key, last_err) in series.iter().zip(series_errors.iter_mut()) {
                        match client.history(&key.0, key.1) {
                            Ok(v) => {
                                shared.lock().unwrap().series.insert(key.clone(), Arc::new(v));
                                *last_err = None;
                            }
                            Err(e) => log_changed(last_err, "home assistant", e),
                        }
                    }
                }
                if let Some(player) = &player {
                    let previous = shared.lock().unwrap().media.clone();
                    let last_err = errors.last_mut().unwrap();
                    match client.media(player, previous.as_ref()) {
                        Ok(v) => {
                            shared.lock().unwrap().media = Some(v);
                            *last_err = None;
                        }
                        Err(e) => log_changed(last_err, "home assistant", e),
                    }
                }
                if !nap(&stop, Duration::from_secs(h.refresh_seconds.max(1))) {
                    break;
                }
            }
        });
    }
    if let Some(h) = cfg.http.clone() {
        http::spawn(h, cfg.pushed(), cfg.table_columns(), Arc::clone(shared), Arc::clone(&stop));
    }
    Sources { stop }
}

pub fn log_changed(last: &mut Option<String>, what: &str, e: anyhow::Error) {
    let msg = format!("{e:#}");
    if last.as_deref() != Some(&msg) {
        eprintln!("panel-ddp: {what}: {msg}");
        *last = Some(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::{records, Value};
    use serde_json::json;

    #[test]
    fn rows_are_objects_of_text_and_series() {
        let rows = records(&json!([{"room": "Kitchen", "temp": 22.8, "n": 3, "on": true, "none": null, "history": [1, 2.5]}])).unwrap();
        let text = |column: &str| match &rows[0][column] {
            Value::Text(s) => s.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!((text("room"), text("temp"), text("n"), text("on"), text("none")), ("Kitchen".into(), "22.8".into(), "3".into(), "true".into(), String::new()));
        assert_eq!(rows[0]["history"], Value::Series(vec![1.0, 2.5]));
        assert_eq!(records(&json!({"a": 1})).unwrap().len(), 1, "one object is one row");
        assert!(records(&json!([])).unwrap().is_empty());
        assert!(records(&json!("rows")).is_err());
        assert!(records(&json!([1])).unwrap_err().contains("row 1"));
        assert!(records(&json!([{"a": 1}, {"h": [1, "x"]}])).unwrap_err().contains("row 2, column h"));
        assert!(records(&json!([{"a": {}}])).is_err());
    }
}
