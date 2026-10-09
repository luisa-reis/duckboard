//! Home Assistant's REST API: entity states for the sensor tiles, their
//! history for the line charts, and the media player's title, artist and
//! album art for the hub. One long-lived
//! access token covers all of it.

use crate::artcache::ArtCache;
use crate::config::HomeAssistantConfig;
use crate::picture::{self, Scaled};
use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct Sensor {
    pub state: String,
    pub unit: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Media {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    /// The art, scaled to where it shows, with the URL it came from so a
    /// repeat of the same track does not fetch it again.
    pub art: Option<Art>,
}

#[derive(Clone, Debug)]
pub struct Art {
    pub url: String,
    /// At every size the art shows at.
    pub scaled: Scaled,
    /// The picture as downloaded, for the art file; shared, since the
    /// snapshot is copied every frame.
    pub original: Option<std::sync::Arc<Vec<u8>>>,
}

/// How many values a history is kept as, evenly spaced over its span.
const HISTORY_POINTS: usize = 256;

/// A past state, as the history API gives it.
#[derive(Deserialize)]
struct Past {
    state: String,
    last_changed: Option<String>,
}

#[derive(Deserialize)]
struct State {
    state: String,
    #[serde(default)]
    attributes: serde_json::Map<String, serde_json::Value>,
}

pub struct Client {
    agent: ureq::Agent,
    base: String,
    token: String,
    cache: ArtCache,
    gamma: f32,
}

impl Client {
    pub fn new(cfg: &HomeAssistantConfig, cache: ArtCache, gamma: f32) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
            base: cfg.url.trim_end_matches('/').to_string(),
            token: cfg.token.clone(),
            cache,
            gamma,
        }
    }

    fn state(&self, entity: &str) -> Result<State> {
        self.agent
            .get(&format!("{}/api/states/{entity}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("GET states/{entity}"))?
            .into_json()
            .with_context(|| format!("parsing states/{entity}"))
    }

    pub fn sensor(&self, entity: &str) -> Result<Sensor> {
        let s = self.state(entity)?;
        let unit = s.attributes.get("unit_of_measurement").and_then(|v| v.as_str()).map(str::to_string);
        Ok(Sensor { state: s.state, unit })
    }

    /// The entity's numeric states over the last `hours`, oldest first, as
    /// `HISTORY_POINTS` evenly spaced values; empty when it had none.
    pub fn history(&self, entity: &str, hours: u32) -> Result<Vec<f64>> {
        let end = Utc::now();
        let start = end - chrono::Duration::hours(hours.into());
        let stamp = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Secs, true);
        let rows: Vec<Vec<Past>> = self
            .agent
            .get(&format!("{}/api/history/period/{}", self.base, stamp(start)))
            .query("filter_entity_id", entity)
            .query("end_time", &stamp(end))
            .query("minimal_response", "")
            .query("no_attributes", "")
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("GET history of {entity}"))?
            .into_json()
            .with_context(|| format!("parsing history of {entity}"))?;
        let span = (end - start).num_milliseconds() as f64;
        let points: Vec<(f64, f64)> = rows
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let value = p.state.parse::<f64>().ok().filter(|v| v.is_finite())?;
                let at = DateTime::parse_from_rfc3339(&p.last_changed?).ok()?.with_timezone(&Utc);
                Some(((at - start).num_milliseconds() as f64 / span, value))
            })
            .collect();
        Ok(spread(&points, HISTORY_POINTS))
    }

    /// The player's current item. `previous` supplies the art to keep when
    /// the picture URL has not changed.
    pub fn media(&self, entity: &str, previous: Option<&Media>) -> Result<Media> {
        let s = self.state(entity)?;
        let text = |key: &str| {
            s.attributes.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
        };
        let picture = s.attributes.get("entity_picture").and_then(|v| v.as_str());
        let art = match picture {
            None => None,
            Some(p) => {
                let url = if p.starts_with("http") { p.to_string() } else { format!("{}{p}", self.base) };
                match previous.and_then(|m| m.art.as_ref()).filter(|a| a.url == url) {
                    Some(kept) => Some(kept.clone()),
                    None => {
                        let (scaled, original) =
                            self.fetch_art(&url, &name_for(&text("media_artist"), &text("media_album_name")))?;
                        Some(Art { scaled, url, original: original.map(std::sync::Arc::new) })
                    }
                }
            }
        };
        Ok(Media {
            playing: s.state == "playing",
            title: text("media_title"),
            artist: text("media_artist"),
            art,
        })
    }

    /// The art for a picture URL: from the cache, or downloaded, decoded
    /// and cached, the original under `name` when there is one.
    fn fetch_art(&self, url: &str, name: &Option<String>) -> Result<crate::artcache::Decoded> {
        if let Some(hit) = self.cache.get(url) {
            return Ok(hit);
        }
        let resp = self
            .agent
            .get(url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .context("GET entity_picture")?;
        let mut bytes = Vec::new();
        std::io::Read::take(resp.into_reader(), 8 << 20)
            .read_to_end(&mut bytes)
            .context("reading entity_picture")?;
        let scaled = picture::decode(&bytes, self.gamma, &self.cache.sizes).context("entity_picture")?;
        if let Err(e) = self.cache.put(url, &scaled).and_then(|()| self.cache.put_original(url, &bytes, name.as_deref())) {
            eprintln!("panel-ddp: art cache: {e:#}");
        }
        Ok((scaled, Some(bytes)))
    }
}

/// `points`, each a place along the span (0 its start, 1 its end) and a
/// value, in time order, as `n` evenly spaced values: the mean of those in
/// each step, a step without any keeping the one before, and the steps
/// before the first point taking its value. Empty without points.
fn spread(points: &[(f64, f64)], n: usize) -> Vec<f64> {
    let Some(&(_, first)) = points.first() else { return Vec::new() };
    let mut steps = vec![(0.0, 0u32); n];
    for &(at, value) in points {
        let step = &mut steps[((at.clamp(0.0, 1.0) * n as f64) as usize).min(n - 1)];
        *step = (step.0 + value, step.1 + 1);
    }
    let mut last = first;
    steps
        .into_iter()
        .map(|(sum, count)| {
            if count > 0 {
                last = sum / f64::from(count);
            }
            last
        })
        .collect()
}

/// "Artist - Album" when both are known, for naming a cached original.
pub fn name_for(artist: &str, album: &str) -> Option<String> {
    match (artist.trim(), album.trim()) {
        ("", _) | (_, "") => None,
        (a, b) => Some(format!("{a} - {b}")),
    }
}

#[cfg(test)]
mod tests {
    use super::spread;

    #[test]
    fn spreads_points_evenly() {
        assert!(spread(&[], 4).is_empty());
        assert_eq!(spread(&[(0.6, 3.0)], 4), [3.0, 3.0, 3.0, 3.0], "one value all along");
        let points = [(0.0, 1.0), (0.1, 3.0), (0.8, 5.0), (1.0, 7.0)];
        assert_eq!(spread(&points, 4), [2.0, 2.0, 2.0, 6.0], "means, kept through empty steps");
    }
}
