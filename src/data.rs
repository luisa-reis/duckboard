//! What the tiles draw from. A `Snapshot` is the latest of everything the
//! sources have fetched; the frame loop takes a copy each frame. The sources
//! run on their own threads and each refreshes its part on its own clock,
//! so a slow or dead service never stalls a frame.

use crate::artcache::ArtCache;
use crate::model::Model;
use crate::picture::Scaled;
use crate::ha::{self, Art, Media, Sensor};
use crate::spotify;
use crate::weather::{self, Weather};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub weather: Option<Weather>,
    /// By entity id.
    pub sensors: HashMap<String, Sensor>,
    pub media: Option<Media>,
}

impl Snapshot {
    /// Made-up data for previews and demos that must not touch the network:
    /// every sensor the config names sweeps 0..100 over `SWEEP_SECONDS` and
    /// holds full for `HOLD_SECONDS`, alerts raise for five seconds of every
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
        let mut sensors: HashMap<String, Sensor> = cfg
            .sensor_entities()
            .into_iter()
            .map(|e| (e, Sensor { state: format!("{value}"), unit: Some("%".into()) }))
            .collect();
        for a in &cfg.alerts {
            let state = if alert_on { a.state.clone() } else { format!("not {}", a.state) };
            sensors.insert(a.entity.clone(), Sensor { state, unit: None });
        }
        let gradient = |w: u32, h: u32| {
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                for x in 0..w {
                    rgb.extend_from_slice(&[(x * 240 / w) as u8, (y * 240 / h) as u8, 180 - (x * 90 / w) as u8]);
                }
            }
            rgb
        };
        let scaled = Scaled::from_fn(&cfg.art_sizes(), gradient);
        Self {
            weather: Some(Weather { temperature: 21.4, code: 61, is_day: true }),
            sensors,
            media: Some(Media {
                playing: true,
                title: "Sample Song Title".into(),
                artist: "Sample Artist".into(),
                art: Some(Art { url: String::new(), scaled, original: None }),
            }),
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
        // With Spotify configured, the media player here is left alone.
        let player = if cfg.spotify.is_some() { None } else { h.media_player.clone() };
        let cache = cache.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let client = ha::Client::new(&h, cache, gamma);
            // One error slot per request, so a missing entity is reported
            // once and a working one next to it does not reset that.
            let mut errors = vec![None; entities.len() + 1];
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
    Sources { stop }
}

pub fn log_changed(last: &mut Option<String>, what: &str, e: anyhow::Error) {
    let msg = format!("{e:#}");
    if last.as_deref() != Some(&msg) {
        eprintln!("panel-ddp: {what}: {msg}");
        *last = Some(msg);
    }
}
