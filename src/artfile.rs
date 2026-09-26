//! The art file: a JPEG kept at the album cover on show, the picture as
//! downloaded, black when no cover is on. A companion view for a laptop
//! beside the panel (see tools/DemoArtViewer).

use crate::config::{Background, HubSpec, Tiles};
use crate::data::Snapshot;
use std::path::PathBuf;

pub struct ArtFile {
    path: PathBuf,
    open: bool,
    /// The URL of the cover the file holds; None is black.
    written: Option<Option<String>>,
}

impl ArtFile {
    pub fn new(path: PathBuf, open: bool) -> Self {
        Self { path, open, written: None }
    }

    /// Rewrites the file when the cover on show changes.
    pub fn update(&mut self, tiles: &Tiles, data: &Snapshot) {
        let shows_art = matches!(tiles.background, Some(Background::Media { .. }))
            || matches!(tiles.hub.spec, HubSpec::Media { .. });
        let art = data.media.as_ref().and_then(|m| m.art.as_ref()).filter(|_| shows_art);
        let key = art.map(|a| a.url.clone());
        if self.written.as_ref() == Some(&key) {
            return;
        }
        let img = match art.and_then(|a| a.original.as_deref()).and_then(|b| image::load_from_memory(b).ok()) {
            Some(orig) => orig.to_rgb8(),
            None => image::RgbImage::new(256, 256),
        };
        // Written whole and renamed into place, so a viewer never reads a
        // half-written file.
        let tmp = self.path.with_extension("jpg.tmp");
        let result = img
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())
            .and_then(|()| std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.written = Some(key);
                if self.open {
                    // Preview re-reads a file it is told to open.
                    let _ = std::process::Command::new("open").arg(&self.path).spawn();
                }
            }
            Err(e) => eprintln!("panel-ddp: art file {}: {e}", self.path.display()),
        }
    }

    /// The file only means something while a run is on.
    pub fn remove(&self) {
        if self.path.exists() {
            if let Err(e) = std::fs::remove_file(&self.path) {
                eprintln!("panel-ddp: removing {}: {e}", self.path.display());
            }
        }
    }
}
