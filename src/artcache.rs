//! A disk cache of decoded album art, keyed by the picture's URL and the
//! gamma it was decoded with, so a changed gamma misses. Each entry
//! is the hub-sized and panel-sized RGB bytes back to back, about 14 KB, so
//! the default cap holds a few hundred covers. Oldest entries go first when
//! the cap is reached; a hit refreshes the entry's time.

use crate::canvas::{HEIGHT, WIDTH};
use crate::mask::HUB;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

const HUB_BYTES: usize = (HUB.width * HUB.height * 3) as usize;
const FULL_BYTES: usize = (WIDTH * HEIGHT * 3) as usize;

#[derive(Clone, Debug)]
pub struct ArtCache {
    dir: PathBuf,
    max_bytes: u64,
    /// Goes into every key; the gamma, so entries decoded differently miss.
    salt: String,
}

impl ArtCache {
    /// A cap of zero disables the cache; nothing is read or written.
    pub fn new(dir: PathBuf, max_bytes: u64, salt: String) -> Self {
        Self { dir, max_bytes, salt }
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(format!("{}\n{url}", self.salt).as_bytes());
        self.dir.join(format!("{:x}.rgb", hash))
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

    /// Deletes the oldest entries until the total is under the cap.
    fn trim(&self) -> Result<()> {
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for e in fs::read_dir(&self.dir).with_context(|| format!("listing {}", self.dir.display()))? {
            let e = e?;
            let p = e.path();
            if p.extension().is_none_or(|x| x != "rgb") {
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
        let c = ArtCache::new(tempdir("rt"), 1 << 20, "g".into());
        assert!(c.get("a").is_none());
        let (h, f) = entry(7);
        c.put("a", &h, &f).unwrap();
        assert_eq!(c.get("a"), Some((h, f)));
        assert!(c.get("b").is_none());
    }

    #[test]
    fn oldest_goes_first_and_a_hit_refreshes() {
        // Room for two entries, not three.
        let c = ArtCache::new(tempdir("ev"), (HUB_BYTES + FULL_BYTES) as u64 * 2, "g".into());
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
        let a = ArtCache::new(dir.clone(), 1 << 20, "2.2".into());
        let b = ArtCache::new(dir, 1 << 20, "1.0".into());
        let (h, f) = entry(3);
        a.put("a", &h, &f).unwrap();
        assert!(a.get("a").is_some());
        assert!(b.get("a").is_none());
    }

    #[test]
    fn zero_cap_disables() {
        let c = ArtCache::new(tempdir("off"), 0, "g".into());
        let (h, f) = entry(1);
        c.put("a", &h, &f).unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.dir.exists());
    }

    #[test]
    fn wrong_size_is_dropped() {
        let c = ArtCache::new(tempdir("bad"), 1 << 20, "g".into());
        fs::create_dir_all(&c.dir).unwrap();
        fs::write(c.path_for("a"), b"short").unwrap();
        assert!(c.get("a").is_none());
        assert!(!c.path_for("a").exists());
    }
}
