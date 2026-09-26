//! What the tiles draw from. A `Snapshot` is the latest of everything the
//! sources have fetched; the frame loop takes a copy each frame. The sources
//! run on their own threads and each refreshes its part on its own clock,
//! so a slow or dead service never stalls a frame.

use crate::config::Config;
use crate::ha::{self, Art, Media, Sensor};
use crate::mask::HUB;
use crate::weather::{self, Weather};
use std::collections::HashMap;
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
    /// every sensor the config names sweeps 0..100 over `SWEEP_FRAMES` and
    /// holds full for a moment, and a gradient plays as album art.
    pub fn sample(cfg: &Config, frame: u32) -> Self {
        const SWEEP_FRAMES: u32 = 150;
        const HOLD_FRAMES: u32 = 30;
        let t = frame % (SWEEP_FRAMES + HOLD_FRAMES);
        let value = (t.min(SWEEP_FRAMES) as f64 * 100.0 / SWEEP_FRAMES as f64 * 10.0).round() / 10.0;
        let sensors = cfg
            .sensor_entities()
            .into_iter()
            .map(|e| (e, Sensor { state: format!("{value}"), unit: Some("%".into()) }))
            .collect();
        let mut rgb = Vec::with_capacity((HUB.width * HUB.height * 3) as usize);
        for y in 0..HUB.height {
            for x in 0..HUB.width {
                rgb.extend_from_slice(&[(x * 11) as u8, (y * 11) as u8, 180 - (x * 4) as u8]);
            }
        }
        Self {
            weather: Some(Weather { temperature: 21.4, code: 61, is_day: true }),
            sensors,
            media: Some(Media {
                playing: true,
                title: "Sample Song Title".into(),
                artist: "Sample Artist".into(),
                art: Some(Art { url: String::new(), rgb }),
            }),
        }
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

/// Starts one thread per configured source. Each keeps the last good value
/// and logs a failure only when its message changes, so a service that is
/// down does not fill the log at every retry.
pub fn spawn_sources(cfg: &Config, shared: &Shared) {
    if let Some(w) = cfg.weather.clone() {
        let shared = Arc::clone(shared);
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
                thread::sleep(Duration::from_secs(60 * w.refresh_minutes.max(1)));
            }
        });
    }

    if let Some(h) = cfg.home_assistant.clone() {
        let shared = Arc::clone(shared);
        let entities = cfg.sensor_entities();
        thread::spawn(move || {
            let client = ha::Client::new(&h);
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
                if let Some(player) = &h.media_player {
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
                thread::sleep(Duration::from_secs(h.refresh_seconds.max(1)));
            }
        });
    }
}

pub fn log_changed(last: &mut Option<String>, what: &str, e: anyhow::Error) {
    let msg = format!("{e:#}");
    if last.as_deref() != Some(&msg) {
        eprintln!("panel-ddp: {what}: {msg}");
        *last = Some(msg);
    }
}
