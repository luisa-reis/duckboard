//! Home Assistant's REST API: entity states for the sensor tiles, and the
//! media player's title, artist and album art for the hub. One long-lived
//! access token covers all of it.

use crate::artcache::ArtCache;
use crate::config::HomeAssistantConfig;
use crate::picture::{self, Scaled};
use anyhow::{Context, Result};
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

/// "Artist - Album" when both are known, for naming a cached original.
pub fn name_for(artist: &str, album: &str) -> Option<String> {
    match (artist.trim(), album.trim()) {
        ("", _) | (_, "") => None,
        (a, b) => Some(format!("{a} - {b}")),
    }
}
