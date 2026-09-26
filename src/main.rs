//! panel-ddp: draws dashboard frames and streams them to a WLED matrix.
//!
//!     panel-ddp run [--config FILE] [--target HOST] [--frames N] [--once] [--sample]
//!     panel-ddp demo [--config FILE] [--once]
//!     panel-ddp frame [--config FILE] [--once]
//!     panel-ddp preview [--config FILE] [--out FILE] [--test | --alert]
//!     panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
//!     panel-ddp spotify-login [--config FILE] [--port N]
//!
//! The config file defaults to dashboard.toml in the current directory.
//! `preview` renders one frame from sample data into a PNG with the mask
//! applied; `--test` renders the test frame instead. HOST defaults to
//! 4.3.2.1 (WLED-AP), PORT to 4048.

mod artcache;
mod artfile;
mod canvas;
mod config;
mod dashboard;
mod data;
mod ddp;
mod demo;
mod frame;
mod ha;
mod hub;
mod icons;
mod mask;
mod pages;
mod palette;
mod spotify;
mod testframe;
mod tiles;
mod weather;

use anyhow::{bail, Context, Result};
use canvas::Canvas;
use config::Config;
use ddp::DdpSender;
use mask::Mask;
use palette::{Palette, Rgba};
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
  panel-ddp demo [--config FILE] [--once]
  panel-ddp frame [--config FILE] [--once]
  panel-ddp preview [--config FILE] [--out FILE] [--test | --weather-code N | --alert]
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
    let cfg = Config::load(&config)?;
    let sp = cfg.spotify.as_ref().context("the config has no [spotify] table")?;
    spotify::login(sp, port)
}

fn cmd_test(args: &[String]) -> Result<()> {
    let s = parse_stream(args)?;
    stream(&s, testframe::draw)
}

/// The `[frame]` pictures, loaded only when a background or a page wants
/// them. For pages a missing folder only leaves those pages out.
fn frame_pictures(cfg: &Config) -> Result<Option<frame::Frame>> {
    let wants = |b: &Option<config::Background>| matches!(b, Some(config::Background::Frame { .. }));
    let pages_want = cfg
        .pages
        .iter()
        .any(|p| matches!(p.background, config::Background::Frame { .. }) || p.data.picture.is_some());
    if cfg.pages.is_empty() && wants(&cfg.tiles.background) {
        return Ok(Some(frame::Frame::new(cfg)?));
    }
    if pages_want {
        return match frame::Frame::new(cfg) {
            Ok(f) => Ok(Some(f)),
            Err(e) => {
                eprintln!("panel-ddp: pages: no [frame] pictures ({e:#})");
                Ok(None)
            }
        };
    }
    Ok(None)
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
    let cfg = Config::load(&config)?;
    if cfg.spotify.is_some() && cfg.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some()) {
        eprintln!("panel-ddp: both [spotify] and [home_assistant].media_player are set; Spotify feeds the hub");
    }
    let pictures = frame_pictures(&cfg)?;
    let pages = if cfg.pages.is_empty() {
        None
    } else {
        let p = pages::Pages::new(&cfg, pictures.as_ref().map_or(0, |f| f.len()))?;
        eprintln!(
            "panel-ddp: {} pages, one pass is {:.0} s",
            cfg.pages.len(),
            p.total_frames() as f32 / cfg.fps as f32
        );
        Some(p)
    };
    if once {
        let p = pages.as_ref().context("--once plays the pages through once; this config has none")?;
        frames = Some(p.total_frames());
    }
    let target = target.unwrap_or_else(|| cfg.target.clone());
    let s = Stream { target: ddp::target_with_default_port(&target), fps: cfg.fps, frames };
    let shared: data::Shared = Default::default();
    if sample {
        eprintln!("panel-ddp: --sample: made-up data, no source is contacted");
    } else {
        data::spawn_sources(&cfg, &shared);
    }
    let palette = Palette::default().with(&cfg.colors);
    let mut art_file = cfg.art_file.clone().map(|p| artfile::ArtFile::new(p, cfg.art_open));
    let result = stream(&s, |c, frame| {
        let mut data = if sample { data::Snapshot::sample(&cfg, frame) } else { shared.lock().unwrap().clone() };
        let (tiles, picture) = match &pages {
            Some(p) => {
                let at = p.at(frame);
                p.apply(at.data, at.t, &mut data);
                let picture = match at.data.picture {
                    Some(i) => pictures.as_ref().map(|f| f.picture_at(i)),
                    None => pictures.as_ref().map(|f| f.picture(frame)),
                };
                (at.tiles, picture)
            }
            None => (&cfg.tiles, pictures.as_ref().map(|f| f.picture(frame))),
        };
        if let Some(a) = art_file.as_mut() {
            a.update(tiles, &data);
        }
        let ctx = tiles::Ctx {
            now: chrono::Local::now(),
            frame,
            data: &data,
            palette: &palette,
            temperature: cfg.temperature,
            fps: cfg.fps,
            picture,
        };
        dashboard::draw(tiles, &cfg.alerts, c, &ctx);
    });
    if let Some(a) = &art_file {
        a.remove();
    }
    result
}

fn cmd_demo(args: &[String]) -> Result<()> {
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
    let cfg = Config::load(&config)?;
    let demo = demo::Demo::new(&cfg);
    let total = demo.total_frames();
    eprintln!("panel-ddp: demo: one pass is {:.0} s", total as f32 / cfg.fps as f32);
    let s = Stream {
        target: ddp::target_with_default_port(&cfg.target),
        fps: cfg.fps,
        frames: once.then_some(total),
    };
    let result = stream(&s, |c, frame| demo.draw(c, frame));
    // The art file is only meaningful while the demo runs.
    if let Some(f) = &cfg.demo.art_file {
        if f.exists() {
            if let Err(e) = std::fs::remove_file(f) {
                eprintln!("panel-ddp: demo: removing {}: {e}", f.display());
            }
        }
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
    let cfg = Config::load(&config)?;
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
    let cfg = Config::load(&config)?;
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
        let palette = Palette::default().with(&cfg.colors);
        let pictures = frame_pictures(&cfg)?;
        let ctx = tiles::Ctx {
            now: chrono::Local::now(),
            frame: 0,
            data: &data,
            palette: &palette,
            temperature: cfg.temperature,
            fps: cfg.fps,
            picture: pictures.as_ref().map(|p| p.picture(0)),
        };
        dashboard::draw(&cfg.tiles, &cfg.alerts, &mut canvas, &ctx);
    }
    mask.preview_png(&canvas, 4, &out)?;
    eprintln!("panel-ddp: wrote {}", out.display());
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
        "demo" => cmd_demo(rest),
        "frame" => cmd_frame(rest),
        "preview" => cmd_preview(rest),
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
