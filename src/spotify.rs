//! Spotify as the source of album art and the now-playing text, straight
//! from the Web API. Uses the PKCE authorisation flow, so the only secret is
//! the refresh token, which `panel-ddp spotify-login` obtains once and the
//! client keeps in a token file, rewriting it as Spotify rotates the token.

use crate::artcache::ArtCache;
use crate::config::SpotifyConfig;
use crate::ha::{decode_art, Art, Media};
use crate::mask::HUB;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const PLAYING_URL: &str = "https://api.spotify.com/v1/me/player/currently-playing?additional_types=track,episode";
const SCOPE: &str = "user-read-currently-playing";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    /// Unix seconds.
    expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn read_tokens(path: &Path) -> Result<Tokens> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!("reading {}; run `panel-ddp spotify-login` first", path.display())
    })?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Written whole and renamed into place, readable by the owner only.
fn write_tokens(path: &Path, t: &Tokens) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(t)?;
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

fn tokens_from(r: TokenResponse, previous_refresh: &str) -> Tokens {
    Tokens {
        access_token: r.access_token,
        refresh_token: r.refresh_token.unwrap_or_else(|| previous_refresh.to_string()),
        expires_at: now() + r.expires_in,
    }
}

pub struct Client {
    agent: ureq::Agent,
    client_id: String,
    token_file: std::path::PathBuf,
    tokens: Tokens,
    cache: ArtCache,
    gamma: f32,
}

impl Client {
    pub fn load(cfg: &SpotifyConfig, cache: ArtCache, gamma: f32) -> Result<Self> {
        Ok(Self {
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
            client_id: cfg.client_id.clone(),
            token_file: cfg.token_file.clone(),
            tokens: read_tokens(&cfg.token_file)?,
            cache,
            gamma,
        })
    }

    /// Refreshes the access token when it is about to expire. Spotify hands
    /// out a new refresh token with each refresh, so the file is rewritten.
    fn ensure_access(&mut self) -> Result<()> {
        if self.tokens.expires_at > now() + 60 {
            return Ok(());
        }
        let r: TokenResponse = self
            .agent
            .post(TOKEN_URL)
            .send_form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &self.tokens.refresh_token),
                ("client_id", &self.client_id),
            ])
            .context("refreshing the Spotify token")?
            .into_json()
            .context("parsing the Spotify token response")?;
        self.tokens = tokens_from(r, &self.tokens.refresh_token);
        write_tokens(&self.token_file, &self.tokens)
    }

    /// What is playing now, or None when nothing is. `previous` supplies the
    /// art to keep when the picture URL has not changed.
    pub fn currently_playing(&mut self, previous: Option<&Media>) -> Result<Option<Media>> {
        self.ensure_access()?;
        let resp = match self
            .agent
            .get(PLAYING_URL)
            .set("Authorization", &format!("Bearer {}", self.tokens.access_token))
            .call()
        {
            Ok(r) => r,
            Err(ureq::Error::Status(401, _)) => {
                self.tokens.expires_at = 0; // refresh on the next round
                bail!("Spotify rejected the access token; refreshing");
            }
            Err(e) => return Err(anyhow!(e).context("GET currently-playing")),
        };
        if resp.status() == 204 {
            return Ok(None);
        }
        let v: serde_json::Value = resp.into_json().context("parsing currently-playing")?;
        let item = &v["item"];
        if item.is_null() {
            return Ok(None);
        }
        let title = item["name"].as_str().unwrap_or("").to_string();
        let artist = match item["artists"].as_array() {
            Some(a) => a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", "),
            None => item["show"]["name"].as_str().unwrap_or("").to_string(),
        };
        // Tracks carry their pictures on the album, episodes on themselves.
        let images = if item["album"]["images"].is_array() { &item["album"]["images"] } else { &item["images"] };
        let url = smallest_image(images);
        let art = match url {
            None => None,
            Some(url) => match previous.and_then(|m| m.art.as_ref()).filter(|a| a.url == url) {
                Some(kept) => Some(kept.clone()),
                None => {
                    let (rgb, full) = self.fetch_art(&url)?;
                    Some(Art { rgb, full, url })
                }
            },
        };
        Ok(Some(Media { playing: v["is_playing"].as_bool().unwrap_or(false), title, artist, art }))
    }

    fn fetch_art(&self, url: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        if let Some(hit) = self.cache.get(url) {
            return Ok(hit);
        }
        let resp = self.agent.get(url).call().context("GET album art")?;
        let mut bytes = Vec::new();
        Read::take(resp.into_reader(), 8 << 20).read_to_end(&mut bytes).context("reading album art")?;
        let art = decode_art(&bytes, self.gamma).context("album art")?;
        if let Err(e) = self.cache.put(url, &art.0, &art.1) {
            eprintln!("panel-ddp: art cache: {e:#}");
        }
        Ok(art)
    }
}

/// The smallest picture that still covers the hub; Spotify offers 640, 300
/// and 64 pixels square.
fn smallest_image(images: &serde_json::Value) -> Option<String> {
    let mut best: Option<(u64, &str)> = None;
    for img in images.as_array()? {
        let (Some(url), Some(w)) = (img["url"].as_str(), img["width"].as_u64()) else { continue };
        if w >= HUB.width as u64 && best.is_none_or(|(bw, _)| w < bw) {
            best = Some((w, url));
        }
    }
    best.map(|(_, u)| u.to_string()).or_else(|| images[0]["url"].as_str().map(str::to_string))
}

fn random_urlsafe(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The one-time login: opens Spotify's consent page, catches the redirect
/// on a local port, exchanges the code and writes the token file.
pub fn login(cfg: &SpotifyConfig, port: u16) -> Result<()> {
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let verifier = random_urlsafe(48)?;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(12)?;
    let url = format!(
        "{AUTH_URL}?client_id={}&response_type=code&redirect_uri={}&scope={}\
         &code_challenge_method=S256&code_challenge={challenge}&state={state}",
        url_encode(&cfg.client_id),
        url_encode(&redirect),
        url_encode(SCOPE)
    );
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("listening on 127.0.0.1:{port} for the redirect"))?;
    eprintln!("The Spotify app must list this redirect URI: {redirect}");
    eprintln!("Open this in a browser and approve:\n\n{url}\n");
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).status();
    eprintln!("Waiting for the redirect ...");

    let (mut stream, _) = listener.accept().context("accepting the redirect")?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).context("reading the redirect")?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut got_state = None;
    let mut error = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("code", v)) => code = Some(v.to_string()),
            Some(("state", v)) => got_state = Some(v.to_string()),
            Some(("error", v)) => error = Some(v.to_string()),
            _ => {}
        }
    }
    let ok = code.is_some() && got_state.as_deref() == Some(&state);
    let body = if ok { "panel-ddp: Spotify login done, you can close this tab." } else { "panel-ddp: login failed, see the terminal." };
    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = stream.flush();
    if let Some(e) = error {
        bail!("Spotify returned: {e}");
    }
    if got_state.as_deref() != Some(&state) {
        bail!("the redirect's state did not match; try again");
    }
    let code = code.context("no code in the redirect")?;

    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let r: TokenResponse = agent
        .post(TOKEN_URL)
        .send_form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("client_id", &cfg.client_id),
            ("code_verifier", &verifier),
        ])
        .context("exchanging the code for tokens")?
        .into_json()
        .context("parsing the token response")?;
    let tokens = tokens_from(r, "");
    if tokens.refresh_token.is_empty() {
        bail!("Spotify returned no refresh token");
    }
    write_tokens(&cfg.token_file, &tokens)?;
    eprintln!("Saved {}", cfg.token_file.display());
    Ok(())
}
