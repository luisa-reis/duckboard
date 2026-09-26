//! panel-ddp: draws dashboard frames and streams them to a WLED matrix.
//!
//!     panel-ddp run [--config FILE] [--frames N] [--sample]
//!     panel-ddp preview [--config FILE] [--out FILE] [--test]
//!     panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
//!
//! The config file defaults to dashboard.toml in the current directory.
//! `preview` renders one frame from sample data into a PNG with the mask
//! applied; `--test` renders the test frame instead. HOST defaults to
//! 4.3.2.1 (WLED-AP), PORT to 4048.

mod canvas;
mod config;
mod dashboard;
mod data;
mod ddp;
mod ha;
mod hub;
mod icons;
mod mask;
mod palette;
mod testframe;
mod tiles;
mod weather;

use anyhow::{bail, Context, Result};
use canvas::Canvas;
use config::Config;
use ddp::DdpSender;
use embedded_graphics::pixelcolor::Rgb888;
use mask::Mask;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const USAGE: &str = "usage:
  panel-ddp run [--config FILE] [--frames N] [--sample]
  panel-ddp preview [--config FILE] [--out FILE] [--test | --weather-code N]
  panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]";

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
    while s.frames.is_none_or(|n| frame < n) {
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

fn cmd_test(args: &[String]) -> Result<()> {
    let s = parse_stream(args)?;
    stream(&s, testframe::draw)
}

fn cmd_run(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut frames = None;
    let mut sample = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--frames" => {
                frames = Some(
                    it.next().and_then(|v| v.parse().ok()).context("--frames needs a number")?,
                )
            }
            "--sample" => sample = true,
            o => bail!("unknown option {o}"),
        }
    }
    let cfg = Config::load(&config)?;
    let s = Stream { target: ddp::target_with_default_port(&cfg.target), fps: cfg.fps, frames };
    let shared: data::Shared = Default::default();
    if sample {
        eprintln!("panel-ddp: --sample: made-up data, no source is contacted");
    } else {
        data::spawn_sources(&cfg, &shared);
    }
    stream(&s, |c, frame| {
        let data = if sample { data::Snapshot::sample(&cfg, frame) } else { shared.lock().unwrap().clone() };
        let ctx = tiles::Ctx { now: chrono::Local::now(), frame, data: &data };
        dashboard::draw(&cfg.tiles, c, &ctx);
    })
}

fn cmd_preview(args: &[String]) -> Result<()> {
    let mut config = PathBuf::from(DEFAULT_CONFIG);
    let mut out = PathBuf::from("preview.png");
    let mut test = false;
    let mut weather_code = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().context("--config needs a file")?.into(),
            "--out" => out = it.next().context("--out needs a file")?.into(),
            "--test" => test = true,
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
    mask::draw_outline(&mut canvas, &mask, Rgb888::new(20, 20, 20));
    if test {
        testframe::draw(&mut canvas, 0);
    } else {
        let mut data = data::Snapshot::sample(&cfg, 95);
        if let (Some(code), Some(w)) = (weather_code, data.weather.as_mut()) {
            w.code = code;
            w.is_day = code < 1000;
            w.code %= 1000;
        }
        let ctx = tiles::Ctx { now: chrono::Local::now(), frame: 0, data: &data };
        dashboard::draw(&cfg.tiles, &mut canvas, &ctx);
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
        "preview" => cmd_preview(rest),
        "test" => cmd_test(rest),
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
