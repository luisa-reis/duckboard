//! A disk cache of decoded album art, keyed by the picture's URL and the
//! gamma it was decoded with, so a changed gamma misses. Each entry
//! is the hub-sized and panel-sized RGB bytes back to back, about 14 KB, so
//! the default cap holds a few hundred covers. With `keep_originals` the
//! picture as downloaded sits beside it, keyed by URL alone and named with
//! its own format's extension. Oldest files go first when the cap is
//! reached; a hit refreshes the entry's time.

use crate::canvas::{HEIGHT, WIDTH};
use crate::mask::HUB;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

const HUB_BYTES: usize = (HUB.width * HUB.height * 3) as usize;
const FULL_BYTES: usize = (WIDTH * HEIGHT * 3) as usize;

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
}

impl ArtCache {
    /// A cap of zero disables the cache; nothing is read or written.
    pub fn new(dir: PathBuf, max_bytes: u64, salt: String, keep_originals: bool) -> Self {
        Self { dir, max_bytes, salt, keep_originals }
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(format!("{}\n{url}", self.salt).as_bytes());
        self.dir.join(format!("{:x}.rgb", hash))
    }

    /// Named by the URL's hash with the picture's own extension: jpg, png,
    /// or whatever the bytes turn out to be.
    fn original_path_for(&self, url: &str, bytes: &[u8]) -> PathBuf {
        let hash = Sha256::digest(url.as_bytes());
        let ext = image::guess_format(bytes).ok().and_then(|f| f.extensions_str().first().copied()).unwrap_or("img");
        self.dir.join(format!("{:x}.{ext}", hash))
    }

    /// Stores a picture as downloaded, when originals are kept.
    pub fn put_original(&self, url: &str, bytes: &[u8]) -> Result<()> {
        if self.max_bytes == 0 || !self.keep_originals {
            return Ok(());
        }
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.original_path_for(url, bytes);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
        self.trim()
    }

    /// The decoded art for `url`, if cached; the entry becomes the newest.
    pub fn get(&self, url: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        if self.max_bytes == 0 {
            return None;
        }
        let path = self.path_for(url);
        let bytes = fs::read(&path).ok()?;
        if bytes.len() != HUB_BYTES + FULL_BYTES {
            let _ = fs::remove_file(&path);
            return None;
        }
        if let Ok(f) = fs::File::open(&path) {
            let _ = f.set_modified(SystemTime::now());
        }
        let (hub, full) = bytes.split_at(HUB_BYTES);
        Some((hub.to_vec(), full.to_vec()))
    }

    /// Stores decoded art and trims the cache back under the cap.
    pub fn put(&self, url: &str, hub: &[u8], full: &[u8]) -> Result<()> {
        if self.max_bytes == 0 {
            return Ok(());
        }
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.path_for(url);
        let tmp = path.with_extension("tmp");
        let mut bytes = Vec::with_capacity(HUB_BYTES + FULL_BYTES);
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
                if b.len() != HUB_BYTES + FULL_BYTES {
                    return None;
                }
                // The original sits beside it under the URL-only hash, which
                // the entry's name does not carry; match on time instead.
                let original = self.original_beside(&p);
                Some(Entry { hub: b[..HUB_BYTES].to_vec(), full: b[HUB_BYTES..].to_vec(), original })
            })
            .collect()
    }

    /// The original written closest in time to a decoded entry, within a
    /// couple of seconds, since both are written by the same fetch.
    fn original_beside(&self, entry: &std::path::Path) -> Option<Vec<u8>> {
        let t = fs::metadata(entry).ok()?.modified().ok()?;
        let mut best: Option<(std::time::Duration, PathBuf)> = None;
        for e in fs::read_dir(&self.dir).ok()?.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x == "rgb" || x == "tmp") {
                continue;
            }
            let m = e.metadata().ok()?.modified().ok()?;
            let d = m.duration_since(t).or_else(|_| t.duration_since(m)).ok()?;
            if d <= std::time::Duration::from_secs(2) && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, p));
            }
        }
        fs::read(best?.1).ok()
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
        let c = ArtCache::new(tempdir("rt"), 1 << 20, "g".into(), false);
        assert!(c.get("a").is_none());
        let (h, f) = entry(7);
        c.put("a", &h, &f).unwrap();
        assert_eq!(c.get("a"), Some((h, f)));
        assert!(c.get("b").is_none());
    }

    #[test]
    fn oldest_goes_first_and_a_hit_refreshes() {
        // Room for two entries, not three.
        let c = ArtCache::new(tempdir("ev"), (HUB_BYTES + FULL_BYTES) as u64 * 2, "g".into(), false);
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
        let a = ArtCache::new(dir.clone(), 1 << 20, "2.2".into(), false);
        let b = ArtCache::new(dir, 1 << 20, "1.0".into(), false);
        let (h, f) = entry(3);
        a.put("a", &h, &f).unwrap();
        assert!(a.get("a").is_some());
        assert!(b.get("a").is_none());
    }

    #[test]
    fn originals_kept_only_when_asked() {
        let off = ArtCache::new(tempdir("orig-off"), 1 << 20, "g".into(), false);
        off.put_original("a", b"jpeg bytes").unwrap();
        assert!(!off.dir.exists());
        let on = ArtCache::new(tempdir("orig-on"), 1 << 20, "g".into(), true);
        let (h, f) = entry(4);
        on.put("a", &h, &f).unwrap();
        let png = {
            let mut buf = std::io::Cursor::new(Vec::new());
            image::RgbImage::new(2, 2).write_to(&mut buf, image::ImageFormat::Png).unwrap();
            buf.into_inner()
        };
        on.put_original("a", &png).unwrap();
        assert!(on.dir.join(format!("{:x}.png", Sha256::digest(b"a"))).exists(), "named by its format");
        let e = on.entries(1);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].original.as_deref(), Some(&png[..]));
    }

    #[test]
    fn zero_cap_disables() {
        let c = ArtCache::new(tempdir("off"), 0, "g".into(), false);
        let (h, f) = entry(1);
        c.put("a", &h, &f).unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.dir.exists());
    }

    #[test]
    fn wrong_size_is_dropped() {
        let c = ArtCache::new(tempdir("bad"), 1 << 20, "g".into(), false);
        fs::create_dir_all(&c.dir).unwrap();
        fs::write(c.path_for("a"), b"short").unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.path_for("a").exists());
    }
}
