//! panel-ddp: draws dashboard frames and streams them to a WLED matrix.
//!
//!     panel-ddp run [--config FILE] [--target HOST] [--frames N] [--once] [--sample]
//!     panel-ddp frame [--config FILE] [--once]
//!     panel-ddp preview [--config FILE] [--out FILE] [--test | --alert]
//!     panel-ddp render [--config FILE] --out DIR [--at SECONDS] [--frames N] [--sample] [--png FRAME]...
//!     panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
//!     panel-ddp spotify-login [--config FILE] [--port N]
//!
//! The config file defaults to dashboard.toml in the current directory; a
//! `.json` file works the same. A config with `pages` loops through them,
//! and `--once` plays them through a single time: `run --config demo.json`
//! is the demo.
//! `preview` renders the first page's first frame from sample data into a
//! PNG with the mask applied; `--test` renders the test frame instead. `render` draws what
//! `run` would send, frame by frame, at a fixed time and without the
//! network, and writes a hash per frame, for checking that a change leaves
//! the output alone. HOST defaults to 4.3.2.1 (WLED-AP), PORT to 4048.

mod artcache;
mod artfile;
mod canvas;
mod config;
mod dashboard;
mod data;
mod ddp;
mod frame;
mod ha;
mod hub;
mod icons;
mod legacy;
mod mask;
mod model;
mod pages;
mod palette;
mod picture;
mod spotify;
mod testframe;
mod tiles;
mod weather;

use anyhow::{bail, Context, Result};
use canvas::Canvas;
use ddp::DdpSender;
use mask::Mask;
use model::Model;
use palette::{Palette, Rgba};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Set by Ctrl-C; the stream loop then ends and the command tidies up.
fn stop_flag() -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let s = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || s.store(true, Ordering::SeqCst));
    stop
}

const USAGE: &str = "usage:
  panel-ddp run [--config FILE] [--target HOST] [--frames N] [--once] [--sample]
  panel-ddp frame [--config FILE] [--once]
  panel-ddp preview [--config FILE] [--out FILE] [--test | --weather-code N | --alert]
  panel-ddp render [--config FILE] --out DIR [--at SECONDS] [--frames N] [--sample] [--png FRAME]...
  panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
  panel-ddp spotify-login [--config FILE] [--port N]";

const DEFAULT_CONFIG: &str = "dashboard.toml";

struct Stream {
    target: String,
    fps: u32,
    frames: Option<u32>,
}

fn parse_stream(args: &[String]) -> Result<Stream> {
    let mut s = Stream { target: "4.3.2.1:4048".into(), fps: 10, frames: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fps" => {
                s.fps = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&v| v > 0)
                    .context("--fps needs a positive number")?
            }
            "--frames" => {
                s.frames = Some(
                    it.next().and_then(|v| v.parse().ok()).context("--frames needs a number")?,
                )
            }
            o if o.starts_with('-') => bail!("unknown option {o}"),
            host => s.target = ddp::target_with_default_port(host),
        }
    }
    Ok(s)
}

/// Runs `draw` for each frame and streams the result, paced from the start
/// time so drawing cost does not drift the rate.
fn stream(s: &Stream, mut draw: impl FnMut(&mut Canvas, u32)) -> Result<()> {
    let mut sender = DdpSender::new(&s.target)
        .with_context(|| format!("opening UDP socket to {}", s.target))?;
    eprintln!(
        "panel-ddp: streaming to {} at {} fps{}",
        s.target,
        s.fps,
        match s.frames {
            Some(n) => format!(", {n} frames"),
            None => ", Ctrl-C to stop".into(),
        }
    );
    let period = Duration::from_secs_f64(1.0 / s.fps as f64);
    let mut canvas = Canvas::new();
    let start = Instant::now();
    let mut frame = 0u32;
    let stop = stop_flag();
    while s.frames.is_none_or(|n| frame < n) && !stop.load(Ordering::SeqCst) {
        draw(&mut canvas, frame);
        sender.send_frame(&canvas.px).context("sending frame")?;
        frame += 1;
        let due = start + period * frame;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    Ok(())
}

fn cmd_spotify_login(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut port = 8888u16;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--port" => port = it.next().and_then(|v| v.parse().ok()).context("--port needs a number")?,
            o => bail!("unknown option {o}"),
        }
    }
    let cfg = legacy::load(&config)?;
    let sp = cfg.spotify.as_ref().context("the config has no [spotify] table")?;
    spotify::login(sp, port)
}

fn cmd_test(args: &[String]) -> Result<()> {
    let s = parse_stream(args)?;
    stream(&s, testframe::draw)
}

/// The `[frame]` pictures, loaded only when a page wants them. A page shown
/// for good needs them; among timed pages a missing folder only leaves
/// those that name a picture out.
fn frame_pictures(cfg: &Model) -> Result<Option<frame::Frame>> {
    if !cfg.wants_pictures() {
        return Ok(None);
    }
    if cfg.is_static() {
        return Ok(Some(frame::Frame::new(cfg)?));
    }
    match frame::Frame::new(cfg) {
        Ok(f) => Ok(Some(f)),
        Err(e) => {
            eprintln!("panel-ddp: pages: no [frame] pictures ({e:#})");
            Ok(None)
        }
    }
}

/// What a run draws from, set up once: the pages, the `[frame]` pictures
/// and the palette.
struct Show<'a> {
    cfg: &'a Model,
    pages: pages::Pages,
    pictures: Option<frame::Frame>,
    palette: Palette,
}

impl<'a> Show<'a> {
    fn new(cfg: &'a Model) -> Result<Self> {
        let pictures = frame_pictures(cfg)?;
        let pages = pages::Pages::new(cfg, pictures.as_ref().map_or(0, |f| f.len()))?;
        if let Some(total) = pages.total_frames() {
            eprintln!("panel-ddp: {} pages, one pass is {:.0} s", cfg.pages.len(), total as f32 / cfg.fps as f32);
        }
        Ok(Self { cfg, pages, pictures, palette: Palette::default().with(&cfg.colors) })
    }

    /// Frames in one pass through the pages, unless a page stays for good.
    fn pass_frames(&self) -> Option<u32> {
        self.pages.total_frames()
    }

    /// Draws `frame`, first laying the page's data over `data`; returns the
    /// page on show.
    fn draw(&self, c: &mut Canvas, frame: u32, now: chrono::DateTime<chrono::Local>, data: &mut data::Snapshot) -> &model::Page {
        let pictures = self.pictures.as_ref();
        let at = self.pages.at(frame);
        self.pages.apply(&at.page.data, at.t, data);
        let picture = match at.page.data.picture {
            Some(i) => pictures.map(|f| f.picture_at(i)),
            None => pictures.map(|f| f.picture(frame)),
        };
        let ctx = tiles::Ctx {
            now,
            frame,
            data,
            palette: &self.palette,
            temperature: self.cfg.temperature,
            fps: self.cfg.fps,
            picture,
        };
        dashboard::draw(at.page, &self.cfg.alerts, self.cfg.alert_area, c, &ctx);
        at.page
    }
}

fn cmd_run(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut frames = None;
    let mut once = false;
    let mut target = None;
    let mut sample = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--target" => target = Some(it.next().context("--target needs a host")?.clone()),
            "--frames" => {
                frames = Some(
                    it.next().and_then(|v| v.parse().ok()).context("--frames needs a number")?,
                )
            }
            "--once" => once = true,
            "--sample" => sample = true,
            o => bail!("unknown option {o}"),
        }
    }
    let cfg = legacy::load(&config)?;
    if cfg.spotify.is_some() && cfg.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some()) {
        eprintln!("panel-ddp: both [spotify] and [home_assistant].media_player are set; Spotify feeds the hub");
    }
    let show = Show::new(&cfg)?;
    if once {
        frames = Some(show.pass_frames().context("--once plays the pages through once; this config has none")?);
    }
    let target = target.unwrap_or_else(|| cfg.target.clone());
    let s = Stream { target: ddp::target_with_default_port(&target), fps: cfg.fps, frames };
    let shared: data::Shared = Default::default();
    if sample {
        eprintln!("panel-ddp: --sample: made-up data, no source is contacted");
    } else {
        data::spawn_sources(&cfg, &shared);
    }
    let mut art_file = cfg.art_file.clone().map(|p| artfile::ArtFile::new(p, cfg.art_open));
    let result = stream(&s, |c, frame| {
        let mut data = if sample { data::Snapshot::sample(&cfg, frame) } else { shared.lock().unwrap().clone() };
        let page = show.draw(c, frame, chrono::Local::now(), &mut data);
        if let Some(a) = art_file.as_mut() {
            a.update(page, &data);
        }
    });
    if let Some(a) = &art_file {
        a.remove();
    }
    result
}

fn cmd_frame(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut once = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--once" => once = true,
            o => bail!("unknown option {o}"),
        }
    }
    let cfg = legacy::load(&config)?;
    let frame = frame::Frame::new(&cfg)?;
    let s = Stream {
        target: ddp::target_with_default_port(&cfg.target),
        fps: cfg.fps,
        frames: once.then_some(frame.total_frames()),
    };
    stream(&s, |c, f| frame.draw(c, f))
}

fn cmd_preview(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut out = PathBuf::from("preview.png");
    let mut test = false;
    let mut alert_preview = false;
    let mut weather_code = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--out" => out = it.next().context("--out needs a file")?.into(),
            "--test" => test = true,
            "--alert" => alert_preview = true,
            "--weather-code" => {
                weather_code = Some(
                    it.next().and_then(|v| v.parse().ok()).context("--weather-code needs a number")?,
                )
            }
            o => bail!("unknown option {o}"),
        }
    }
    let cfg = legacy::load(&config)?;
    let mask = if cfg.gaps.exists() {
        Mask::load(&cfg.gaps)?
    } else {
        eprintln!("panel-ddp: no gap file at {}; the preview shows the whole panel", cfg.gaps.display());
        Mask::none()
    };
    let mut canvas = Canvas::new();
    mask::draw_outline(&mut canvas, &mask, Rgba::rgb(20, 20, 20));
    if test {
        testframe::draw(&mut canvas, 0);
    } else {
        let mut data = data::Snapshot::sample(&cfg, if alert_preview { 60 } else { 0 });
        if let (Some(code), Some(w)) = (weather_code, data.weather.as_mut()) {
            w.code = code;
            w.is_day = code < 1000;
            w.code %= 1000;
        }
        // The first page, at its first frame.
        Show::new(&cfg)?.draw(&mut canvas, 0, chrono::Local::now(), &mut data);
    }
    mask.preview_png(&canvas, 4, &out)?;
    eprintln!("panel-ddp: wrote {}", out.display());
    Ok(())
}

/// Draws what `run` would send without sending it: the clock starts at
/// `--at` (seconds since 1970, a fixed default) and moves on with the
/// frames, and the data is empty (pages bring their own) or, with
/// `--sample`, made up. Nothing is fetched and no art file is written.
/// Writes `frames.txt`, one line per frame with its SHA-256, and a PNG of
/// each frame asked for with `--png`.
fn cmd_render(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut out = None;
    let mut at: i64 = 1_790_000_000;
    let mut frames = None;
    let mut sample = false;
    let mut pngs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a folder")?)),
            "--at" => at = it.next().and_then(|v| v.parse().ok()).context("--at needs seconds since 1970")?,
            "--frames" => frames = Some(it.next().and_then(|v| v.parse().ok()).context("--frames needs a number")?),
            "--sample" => sample = true,
            "--png" => pngs.push(it.next().and_then(|v| v.parse::<u32>().ok()).context("--png needs a frame number")?),
            o => bail!("unknown option {o}"),
        }
    }
    let out = out.context("render needs --out DIR")?;
    let cfg = legacy::load(&config)?;
    let show = Show::new(&cfg)?;
    let frames = frames.or(show.pass_frames()).context("a config without pages needs --frames")?;
    let start = chrono::DateTime::from_timestamp(at, 0).context("--at is out of range")?;
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let mut list = String::new();
    let mut canvas = Canvas::new();
    for frame in 0..frames {
        let now = start + Duration::from_secs_f64(frame as f64 / cfg.fps as f64);
        let mut data = if sample { data::Snapshot::sample(&cfg, frame) } else { data::Snapshot::default() };
        show.draw(&mut canvas, frame, now.with_timezone(&chrono::Local), &mut data);
        list.push_str(&format!("{frame} {:x}\n", Sha256::digest(&canvas.px)));
        if pngs.contains(&frame) {
            let png = out.join(format!("frame-{frame:05}.png"));
            Mask::none().preview_png(&canvas, 4, &png)?;
        }
    }
    let path = out.join("frames.txt");
    std::fs::write(&path, list).with_context(|| format!("writing {}", path.display()))?;
    eprintln!("panel-ddp: rendered {frames} frames into {}", out.display());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let result = match cmd {
        "run" => cmd_run(rest),
        "frame" => cmd_frame(rest),
        "preview" => cmd_preview(rest),
        "render" => cmd_render(rest),
        "test" => cmd_test(rest),
        "spotify-login" => cmd_spotify_login(rest),
        "-h" | "--help" => {
            eprintln!("{USAGE}");
            return;
        }
        other => Err(anyhow::anyhow!("unknown command {other}")),
    };
    if let Err(e) = result {
        eprintln!("panel-ddp: {e:#}");
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
}
