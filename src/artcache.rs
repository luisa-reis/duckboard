//! A disk cache of decoded album art, keyed by the picture's URL, the gamma
//! it was decoded with and the hub's size, so a change of either misses. Each
//! entry is the hub-sized and panel-sized RGB bytes back to back, about 14 KB
//! with the default hub, so
//! the default cap holds a few hundred covers. With `keep_originals` the
//! picture as downloaded sits beside it, named "Artist - Album" when known
//! and by the URL's hash otherwise, with its own format's extension. Oldest files go first when the cap is
//! reached; a hit refreshes the entry's time.

use crate::canvas::{HEIGHT, WIDTH};
use crate::config::Config;
use anyhow::{Context, Result};
use embedded_graphics::prelude::Size;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

const FULL_BYTES: usize = (WIDTH * HEIGHT * 3) as usize;

/// A name made safe for a file: path separators and control characters
/// replaced, ends trimmed, and no longer than 120 characters.
fn safe_file_stem(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' || c.is_control() { '_' } else { c })
        .collect();
    s = s.trim().trim_matches('.').to_string();
    if s.chars().count() > 120 {
        s = s.chars().take(120).collect();
    }
    s
}

/// Hub pixels, panel pixels, and the original when one was kept.
pub type Decoded = (Vec<u8>, Vec<u8>, Option<Vec<u8>>);

/// A cached cover.
pub struct Entry {
    pub hub: Vec<u8>,
    pub full: Vec<u8>,
    /// The picture as downloaded, when originals are kept.
    pub original: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ArtCache {
    dir: PathBuf,
    max_bytes: u64,
    /// Goes into every decoded entry's key; the gamma, so entries decoded
    /// differently miss.
    salt: String,
    pub keep_originals: bool,
    /// The size hub art is decoded at, the hub region's.
    pub hub: Size,
}

impl ArtCache {
    /// A cap of zero disables the cache; nothing is read or written.
    pub fn new(dir: PathBuf, max_bytes: u64, salt: String, keep_originals: bool, hub: Size) -> Self {
        Self { dir, max_bytes, salt, keep_originals, hub }
    }

    /// The cache a config describes, for art decoded with its gamma at its
    /// hub's size.
    pub fn for_config(cfg: &Config) -> Self {
        Self::new(
            cfg.art_cache.dir.clone(),
            cfg.art_cache.max_bytes(),
            format!("gamma {}", cfg.gamma),
            cfg.art_cache.keep_originals,
            cfg.regions.hub.size(),
        )
    }

    fn hub_bytes(&self) -> usize {
        (self.hub.width * self.hub.height * 3) as usize
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(format!("{}\n{}x{}\n{url}", self.salt, self.hub.width, self.hub.height).as_bytes());
        self.dir.join(format!("{:x}.rgb", hash))
    }

    /// Named "Artist - Album" when given a name, by the URL's hash
    /// otherwise, with the picture's own extension: jpg, png, or whatever
    /// the bytes turn out to be.
    fn original_path_for(&self, url: &str, bytes: &[u8], name: Option<&str>) -> PathBuf {
        let stem = match name.map(safe_file_stem).filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => format!("{:x}", Sha256::digest(url.as_bytes())),
        };
        let ext = image::guess_format(bytes).ok().and_then(|f| f.extensions_str().first().copied()).unwrap_or("img");
        self.dir.join(format!("{stem}.{ext}"))
    }

    /// Stores a picture as downloaded, when originals are kept, under
    /// `name` if there is one.
    pub fn put_original(&self, url: &str, bytes: &[u8], name: Option<&str>) -> Result<()> {
        if self.max_bytes == 0 || !self.keep_originals {
            return Ok(());
        }
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.original_path_for(url, bytes, name);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
        self.trim()
    }

    /// The decoded art for `url`, if cached; the entry becomes the newest.
    /// The hub and panel pixels for `url`, and the original when one was
    /// kept. Both files become the newest, so they are evicted together.
    pub fn get(&self, url: &str) -> Option<Decoded> {
        if self.max_bytes == 0 {
            return None;
        }
        let path = self.path_for(url);
        let bytes = fs::read(&path).ok()?;
        if bytes.len() != self.hub_bytes() + FULL_BYTES {
            let _ = fs::remove_file(&path);
            return None;
        }
        // Found before the touch, since pairing goes by modification time.
        let original_path = self.original_path_beside(&path);
        let original = original_path.as_ref().and_then(|p| fs::read(p).ok());
        let now = SystemTime::now();
        for p in std::iter::once(&path).chain(original_path.as_ref()) {
            if let Ok(f) = fs::File::open(p) {
                let _ = f.set_modified(now);
            }
        }
        let (hub, full) = bytes.split_at(self.hub_bytes());
        Some((hub.to_vec(), full.to_vec(), original))
    }

    /// Stores decoded art and trims the cache back under the cap.
    pub fn put(&self, url: &str, hub: &[u8], full: &[u8]) -> Result<()> {
        if self.max_bytes == 0 {
            return Ok(());
        }
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.path_for(url);
        let tmp = path.with_extension("tmp");
        let mut bytes = Vec::with_capacity(self.hub_bytes() + FULL_BYTES);
        bytes.extend_from_slice(hub);
        bytes.extend_from_slice(full);
        fs::write(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
        self.trim()
    }

    /// Up to `limit` cached covers, newest first: the hub and panel pixels
    /// and, when kept, the picture as downloaded.
    pub fn entries(&self, limit: usize) -> Vec<Entry> {
        let mut files: Vec<(SystemTime, PathBuf)> = match fs::read_dir(&self.dir) {
            Ok(rd) => rd
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "rgb"))
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect(),
            Err(_) => return Vec::new(),
        };
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        files
            .into_iter()
            .take(limit)
            .filter_map(|(_, p)| {
                let b = fs::read(&p).ok()?;
                if b.len() != self.hub_bytes() + FULL_BYTES {
                    return None;
                }
                // The original sits beside it under the URL-only hash, which
                // the entry's name does not carry; match on time instead.
                let original = self.original_beside(&p);
                let (hub, full) = b.split_at(self.hub_bytes());
                Some(Entry { hub: hub.to_vec(), full: full.to_vec(), original })
            })
            .collect()
    }

    /// The original written closest in time to a decoded entry, within a
    /// couple of seconds, since both are written by the same fetch.
    fn original_beside(&self, entry: &std::path::Path) -> Option<Vec<u8>> {
        fs::read(self.original_path_beside(entry)?).ok()
    }

    fn original_path_beside(&self, entry: &std::path::Path) -> Option<PathBuf> {
        let t = fs::metadata(entry).ok()?.modified().ok()?;
        let mut best: Option<(std::time::Duration, PathBuf)> = None;
        for e in fs::read_dir(&self.dir).ok()?.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x == "rgb" || x == "tmp") || p.file_name().is_some_and(|n| n == ".DS_Store") {
                continue;
            }
            let m = e.metadata().ok()?.modified().ok()?;
            let d = m.duration_since(t).or_else(|_| t.duration_since(m)).ok()?;
            if d <= std::time::Duration::from_secs(2) && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, p));
            }
        }
        Some(best?.1)
    }

    /// Deletes the oldest files, entries and originals alike, until the
    /// total is under the cap.
    fn trim(&self) -> Result<()> {
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for e in fs::read_dir(&self.dir).with_context(|| format!("listing {}", self.dir.display()))? {
            let e = e?;
            let p = e.path();
            if p.extension().is_none_or(|x| x == "tmp") {
                continue;
            }
            let m = e.metadata()?;
            entries.push((m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), p));
        }
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        entries.sort();
        for (_, len, p) in entries {
            if total <= self.max_bytes {
                break;
            }
            fs::remove_file(&p).with_context(|| format!("removing {}", p.display()))?;
            total -= len;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUB: Size = Size::new(22, 22);
    const HUB_BYTES: usize = (HUB.width * HUB.height * 3) as usize;

    fn entry(n: u8) -> (Vec<u8>, Vec<u8>) {
        (vec![n; HUB_BYTES], vec![n; FULL_BYTES])
    }

    fn tempdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("panel-ddp-artcache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn round_trip_and_miss() {
        let c = ArtCache::new(tempdir("rt"), 1 << 20, "g".into(), false, HUB);
        assert!(c.get("a").is_none());
        let (h, f) = entry(7);
        c.put("a", &h, &f).unwrap();
        assert_eq!(c.get("a"), Some((h, f, None)));
        assert!(c.get("b").is_none());
    }

    #[test]
    fn oldest_goes_first_and_a_hit_refreshes() {
        // Room for two entries, not three.
        let c = ArtCache::new(tempdir("ev"), (HUB_BYTES + FULL_BYTES) as u64 * 2, "g".into(), false, HUB);
        for (i, url) in ["a", "b"].iter().enumerate() {
            let (h, f) = entry(i as u8);
            c.put(url, &h, &f).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Touch "a" so "b" is now the oldest.
        assert!(c.get("a").is_some());
        std::thread::sleep(std::time::Duration::from_millis(20));
        let (h, f) = entry(2);
        c.put("c", &h, &f).unwrap();
        assert!(c.get("a").is_some(), "refreshed entry kept");
        assert!(c.get("b").is_none(), "oldest entry evicted");
        assert!(c.get("c").is_some());
    }

    #[test]
    fn salt_separates_entries() {
        let dir = tempdir("salt");
        let a = ArtCache::new(dir.clone(), 1 << 20, "2.2".into(), false, HUB);
        let b = ArtCache::new(dir, 1 << 20, "1.0".into(), false, HUB);
        let (h, f) = entry(3);
        a.put("a", &h, &f).unwrap();
        assert!(a.get("a").is_some());
        assert!(b.get("a").is_none());
    }

    #[test]
    fn hub_size_separates_entries() {
        let dir = tempdir("hub");
        let small = ArtCache::new(dir.clone(), 1 << 20, "g".into(), false, HUB);
        let big = ArtCache::new(dir, 1 << 20, "g".into(), false, Size::new(30, 20));
        let (h, f) = entry(5);
        small.put("a", &h, &f).unwrap();
        assert!(big.get("a").is_none(), "a different hub misses");
        let h2 = vec![6; 30 * 20 * 3];
        big.put("a", &h2, &f).unwrap();
        assert_eq!(small.get("a").map(|e| e.0), Some(h), "and does not evict the other size");
        assert_eq!(big.get("a").map(|e| e.0), Some(h2));
        assert_eq!(big.entries(8).iter().filter(|e| e.hub.len() == 30 * 20 * 3).count(), 1);
    }

    #[test]
    fn originals_kept_only_when_asked() {
        let off = ArtCache::new(tempdir("orig-off"), 1 << 20, "g".into(), false, HUB);
        off.put_original("a", b"jpeg bytes", None).unwrap();
        assert!(!off.dir.exists());
        let on = ArtCache::new(tempdir("orig-on"), 1 << 20, "g".into(), true, HUB);
        let (h, f) = entry(4);
        on.put("a", &h, &f).unwrap();
        let png = {
            let mut buf = std::io::Cursor::new(Vec::new());
            image::RgbImage::new(2, 2).write_to(&mut buf, image::ImageFormat::Png).unwrap();
            buf.into_inner()
        };
        on.put_original("a", &png, None).unwrap();
        assert!(on.dir.join(format!("{:x}.png", Sha256::digest(b"a"))).exists(), "hash name, its format's extension");
        on.put_original("b", &png, Some("AC/DC - Back in Black")).unwrap();
        assert!(on.dir.join("AC_DC - Back in Black.png").exists(), "named after the album, made safe");
        let e = on.entries(1);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].original.as_deref(), Some(&png[..]));
    }

    #[test]
    fn zero_cap_disables() {
        let c = ArtCache::new(tempdir("off"), 0, "g".into(), false, HUB);
        let (h, f) = entry(1);
        c.put("a", &h, &f).unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.dir.exists());
    }

    #[test]
    fn wrong_size_is_dropped() {
        let c = ArtCache::new(tempdir("bad"), 1 << 20, "g".into(), false, HUB);
        fs::create_dir_all(&c.dir).unwrap();
        fs::write(c.path_for("a"), b"short").unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.path_for("a").exists());
    }
}
