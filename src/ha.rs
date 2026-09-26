//! Home Assistant's REST API: entity states for the sensor tiles, and the
//! media player's title, artist and album art for the hub. One long-lived
//! access token covers all of it.

use crate::artcache::ArtCache;
use crate::canvas::{HEIGHT, WIDTH};
use crate::config::HomeAssistantConfig;
use crate::mask::HUB;
use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
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
    /// The art, resized to the hub, with the URL it came from so a repeat
    /// of the same track does not fetch it again.
    pub art: Option<Art>,
}

#[derive(Clone, Debug)]
pub struct Art {
    pub url: String,
    /// Scaled to the hub.
    pub rgb: Vec<u8>,
    /// Scaled to the whole panel, for the background.
    pub full: Vec<u8>,
}

/// Decodes a picture and scales it to fill the hub and the panel, cropping
/// to square.
pub fn decode_art(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let img = image::load_from_memory(bytes).map_err(|e| anyhow!("decoding picture: {e}"))?;
    let hub = img.resize_to_fill(HUB.width, HUB.height, FilterType::Lanczos3).to_rgb8().into_raw();
    let full = img.resize_to_fill(WIDTH, HEIGHT, FilterType::Lanczos3).to_rgb8().into_raw();
    Ok((hub, full))
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
}

impl Client {
    pub fn new(cfg: &HomeAssistantConfig, cache: ArtCache) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
            base: cfg.url.trim_end_matches('/').to_string(),
            token: cfg.token.clone(),
            cache,
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
                        let (rgb, full) = self.fetch_art(&url)?;
                        Some(Art { rgb, full, url })
                    }
                }
            }
        };
        Ok(Media { playing: s.state == "playing", title: text("media_title"), artist: text("media_artist"), art })
    }

    /// The art for a picture URL: from the cache, or downloaded, decoded
    /// and cached.
    fn fetch_art(&self, url: &str) -> Result<(Vec<u8>, Vec<u8>)> {
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
        let art = decode_art(&bytes).context("entity_picture")?;
        if let Err(e) = self.cache.put(url, &art.0, &art.1) {
            eprintln!("panel-ddp: art cache: {e:#}");
        }
        Ok(art)
    }
}
