//! What the tiles draw from. A `Snapshot` is the latest of everything the
//! sources have fetched; the frame loop takes a copy each frame. The sources
//! run on their own threads and each refreshes its part on its own clock,
//! so a slow or dead service never stalls a frame.

use crate::config::Config;
use crate::weather::{self, Weather};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub weather: Option<Weather>,
}

impl Snapshot {
    /// Made-up data for previews that must not touch the network.
    pub fn sample() -> Self {
        Self { weather: Some(Weather { temperature: 21.4, code: 61, is_day: true }) }
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
}

pub fn log_changed(last: &mut Option<String>, what: &str, e: anyhow::Error) {
    let msg = format!("{e:#}");
    if last.as_deref() != Some(&msg) {
        eprintln!("panel-ddp: {what}: {msg}");
        *last = Some(msg);
    }
}
