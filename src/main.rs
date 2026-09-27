//! panel-ddp: draws dashboard frames and streams them to a WLED matrix.
//!
//!     panel-ddp run [--config FILE] [--target HOST] [--frames N] [--once] [--sample]
//!     panel-ddp preview [--config FILE] [--out FILE] [--test | --alert]
//!     panel-ddp render [--config FILE] --out DIR [--at SECONDS] [--frames N] [--sample] [--png FRAME]...
//!     panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
//!     panel-ddp spotify-login [--config FILE] [--port N]
//!     panel-ddp check FILE...
//!     panel-ddp migrate OLD [--out NEW.yaml] [--secrets FILE]
//!     panel-ddp schema
//!
//! The config file, YAML, defaults to dashboard.yaml in the current
//! directory. `run` plays the playlist its schedule picks, reloading the
//! file when it changes; `--once` plays that playlist through a single
//! time: `run --config demo.yaml --once` is the demo. `preview` renders the
//! first page's first frame from sample data into a PNG with the mask
//! applied; `--test` renders the test frame instead. `render` draws what
//! `run` would send, frame by frame, at a fixed time and without the
//! network, and writes a hash per frame, for checking that a change leaves
//! the output alone. `check` loads configs, `schema` prints their JSON
//! Schema, and `migrate` converts an older TOML or JSON config. HOST
//! defaults to 4.3.2.1 (WLED-AP), PORT to 4048.

mod art;
mod artcache;
mod artfile;
mod canvas;
mod config;
mod dashboard;
mod data;
mod ddp;
mod format;
mod frame;
mod ha;
mod icons;
mod legacy;
mod mask;
mod migrate;
mod model;
mod pages;
mod palette;
mod picture;
mod secrets;
mod spotify;
mod testframe;
mod tiles;
mod weather;

use anyhow::{bail, Context, Result};
use canvas::Canvas;
use ddp::DdpSender;
use mask::Mask;
use model::Model;
use palette::Rgba;
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
  panel-ddp preview [--config FILE] [--out FILE] [--test | --weather-code N | --alert]
  panel-ddp render [--config FILE] --out DIR [--at SECONDS] [--frames N] [--sample] [--png FRAME]...
  panel-ddp test [HOST[:PORT]] [--fps N] [--frames N]
  panel-ddp spotify-login [--config FILE] [--port N]
  panel-ddp check FILE...
  panel-ddp migrate OLD [--out NEW.yaml] [--secrets FILE]
  panel-ddp schema";

const DEFAULT_CONFIG: &str = "dashboard.yaml";

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

/// Runs `draw` for each frame and streams what it drew, paced from the
/// start time so drawing cost does not drift the rate. A frame `draw`
/// returns false for is not sent, so the board falls back to its presets
/// while it stays so.
fn stream(s: &Stream, mut draw: impl FnMut(&mut Canvas, u32) -> bool) -> Result<()> {
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
        if draw(&mut canvas, frame) {
            sender.send_frame(&canvas.px).context("sending frame")?;
        }
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
    let cfg = model::load(&config)?;
    let sp = cfg.spotify.as_ref().context("the config has no [spotify] table")?;
    spotify::login(sp, port)
}

fn cmd_test(args: &[String]) -> Result<()> {
    let s = parse_stream(args)?;
    stream(&s, |c, frame| {
        testframe::draw(c, frame);
        true
    })
}

/// The `[frame]` pictures, loaded only when a page wants them. A page shown
/// for good needs them; among timed pages a missing folder only leaves
/// those that name a picture out.
fn frame_pictures(cfg: &Model) -> Result<Option<frame::Frame>> {
    if !cfg.wants_pictures() {
        return Ok(None);
    }
    let sizes = cfg.picture_sizes();
    if cfg.is_static() {
        return Ok(Some(frame::Frame::new(cfg, &sizes)?));
    }
    match frame::Frame::new(cfg, &sizes) {
        Ok(f) => Ok(Some(f)),
        Err(e) => {
            eprintln!("panel-ddp: pages: no [frame] pictures ({e:#})");
            Ok(None)
        }
    }
}

/// What a run draws from, set up once: the model, its pages and the
/// `[frame]` pictures.
struct Show {
    cfg: Model,
    pages: pages::Pages,
    pictures: Option<frame::Frame>,
}

impl Show {
    fn new(cfg: Model) -> Result<Self> {
        let pictures = frame_pictures(&cfg)?;
        let pages = pages::Pages::new(&cfg, pictures.as_ref().map_or(0, |f| f.len()))?;
        Ok(Self { cfg, pages, pictures })
    }

    /// Frames in one pass through the playlist playing at `now`, unless a
    /// page stays for good or nothing plays.
    fn pass_frames(&self, now: &chrono::DateTime<chrono::Local>) -> Option<u32> {
        let frames = self.pages.pass_frames(now)?;
        eprintln!("panel-ddp: one pass is {:.0} s", frames as f32 / self.cfg.fps as f32);
        Some(frames)
    }

    /// Draws `frame`, first laying the page's data over `data`; returns the
    /// page on show, or None, drawing nothing, while nothing is scheduled.
    fn draw(&self, c: &mut Canvas, frame: u32, now: chrono::DateTime<chrono::Local>, data: &mut data::Snapshot) -> Option<&model::Page> {
        let pictures = self.pictures.as_ref();
        let at = self.pages.at(frame, &now)?;
        self.pages.apply(&at.page.data, at.t, data);
        let picture = match at.page.data.picture {
            Some(i) => pictures.map(|f| f.picture_at(i)),
            None => pictures.map(|f| f.picture(frame)),
        };
        let ctx = tiles::Ctx {
            now,
            frame,
            data,
            temperature: self.cfg.temperature,
            fps: self.cfg.fps,
            picture,
        };
        dashboard::draw(at.page, &self.cfg.alerts, self.cfg.alert_area, c, &ctx);
        Some(at.page)
    }
}

/// What the sources depend on: while it stays the same across a reload,
/// the running sources and what they fetched are kept.
fn sources_key(m: &Model) -> String {
    format!(
        "{:?}",
        (&m.weather, &m.spotify, &m.home_assistant, &m.art_cache, m.gamma, m.art_sizes(), m.sensor_entities())
    )
}

/// A running show and the sources feeding it, replaced as a whole when the
/// configuration changes.
struct Live {
    show: Show,
    shared: data::Shared,
    /// None with made-up data.
    sources: Option<data::Sources>,
    art_file: Option<artfile::ArtFile>,
}

impl Live {
    fn new(cfg: Model, sample: bool) -> Result<Self> {
        let shared: data::Shared = Default::default();
        let sources = (!sample).then(|| data::spawn_sources(&cfg, &shared));
        let art_file = cfg.art_file.clone().map(|p| artfile::ArtFile::new(p, cfg.art_open));
        Ok(Self { show: Show::new(cfg)?, shared, sources, art_file })
    }

    /// Switches to `cfg`, keeping the sources when nothing they depend on
    /// changed. The target and the frame rate stay those of the start.
    fn reload(&mut self, cfg: Model, sample: bool) -> Result<()> {
        let old = &self.show.cfg;
        if cfg.target != old.target || cfg.fps != old.fps {
            eprintln!("panel-ddp: target and fps apply at the next start");
        }
        let same_sources = sources_key(&cfg) == sources_key(old);
        let same_art_file = (&cfg.art_file, cfg.art_open) == (&old.art_file, old.art_open);
        let show = Show::new(cfg)?;
        if !same_sources && !sample {
            let shared: data::Shared = Default::default();
            // The old sources stop as their handle goes.
            self.sources = Some(data::spawn_sources(&show.cfg, &shared));
            self.shared = shared;
        }
        if !same_art_file {
            if let Some(a) = &self.art_file {
                a.remove();
            }
            self.art_file = show.cfg.art_file.clone().map(|p| artfile::ArtFile::new(p, show.cfg.art_open));
        }
        self.show = show;
        Ok(())
    }
}

/// The modification times of a model's files, to notice a change.
struct Watch {
    files: Vec<(PathBuf, Option<std::time::SystemTime>)>,
}

impl Watch {
    fn new(files: &[PathBuf]) -> Self {
        Self { files: files.iter().map(|f| (f.clone(), modified(f))).collect() }
    }

    /// Whether any file changed (or appeared, or went) since last asked.
    fn changed(&mut self) -> bool {
        let mut changed = false;
        for (f, seen) in &mut self.files {
            let now = modified(f);
            if now != *seen {
                *seen = now;
                changed = true;
            }
        }
        changed
    }
}

fn modified(f: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(f).and_then(|m| m.modified()).ok()
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
    let cfg = model::load(&config)?;
    if cfg.spotify.is_some() && cfg.home_assistant.as_ref().is_some_and(|h| h.media_player.is_some()) {
        eprintln!("panel-ddp: both Spotify and a Home Assistant media player are set; Spotify feeds the art");
    }
    if sample {
        eprintln!("panel-ddp: --sample: made-up data, no source is contacted");
    }
    let target = target.unwrap_or_else(|| cfg.target.clone());
    let fps = cfg.fps;
    let mut watch = Watch::new(&cfg.files);
    let mut live = Live::new(cfg, sample)?;
    if once {
        let pass = live.show.pass_frames(&chrono::Local::now());
        frames = Some(pass.context("--once plays the playlist on now through once; this config has a page shown for good, or nothing on now")?);
    }
    let s = Stream { target: ddp::target_with_default_port(&target), fps, frames };
    let result = stream(&s, |c, frame| {
        // About once a second, a changed configuration that loads is
        // switched to; one that does not is reported and waited out.
        if frame % fps == 0 && frame > 0 && watch.changed() {
            match model::load(&config) {
                Ok(cfg) => {
                    let files = cfg.files.clone();
                    match live.reload(cfg, sample) {
                        Ok(()) => {
                            eprintln!("panel-ddp: reloaded {}", config.display());
                            watch = Watch::new(&files);
                        }
                        Err(e) => eprintln!("panel-ddp: {} not reloaded: {e:#}", config.display()),
                    }
                }
                Err(e) => eprintln!("panel-ddp: {} not reloaded: {e:#}", config.display()),
            }
        }
        let mut data =
            if sample { data::Snapshot::sample(&live.show.cfg, frame) } else { live.shared.lock().unwrap().clone() };
        let Some(page) = live.show.draw(c, frame, chrono::Local::now(), &mut data) else { return false };
        if let Some(a) = live.art_file.as_mut() {
            a.update(page, &data);
        }
        true
    });
    if let Some(a) = &live.art_file {
        a.remove();
    }
    result
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
    let cfg = model::load(&config)?;
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
        // Six seconds in, an alert of the made-up data is up.
        let mut data = data::Snapshot::sample(&cfg, if alert_preview { 6 * cfg.fps } else { 0 });
        if let (Some(code), Some(w)) = (weather_code, data.weather.as_mut()) {
            w.code = code;
            w.is_day = code < 1000;
            w.code %= 1000;
        }
        // The first page scheduled now, at its first frame.
        if Show::new(cfg)?.draw(&mut canvas, 0, chrono::Local::now(), &mut data).is_none() {
            eprintln!("panel-ddp: nothing is scheduled now; the preview is dark");
        }
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
    let cfg = model::load(&config)?;
    let fps = cfg.fps;
    let show = Show::new(cfg)?;
    let start = chrono::DateTime::from_timestamp(at, 0).context("--at is out of range")?;
    let frames = frames
        .or_else(|| show.pass_frames(&start.with_timezone(&chrono::Local)))
        .context("without a pass to play (a page shown for good, or nothing on at --at), render needs --frames")?;
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let mut list = String::new();
    let mut canvas = Canvas::new();
    for frame in 0..frames {
        let now = start + Duration::from_secs_f64(frame as f64 / fps as f64);
        let mut data = if sample { data::Snapshot::sample(&show.cfg, frame) } else { data::Snapshot::default() };
        if show.draw(&mut canvas, frame, now.with_timezone(&chrono::Local), &mut data).is_none() {
            list.push_str(&format!("{frame} dark\n"));
            continue;
        }
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

/// Loads each file as `run` would and says what it holds, or what is wrong
/// with it; fails if any is wrong.
fn cmd_check(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("check needs a file");
    }
    let mut bad = 0;
    for f in args {
        match model::load(std::path::Path::new(f)) {
            Ok(m) => eprintln!(
                "{f}: ok, {} pages, {} playlists, {} schedule rules",
                m.pages.len(),
                m.playlists.len(),
                m.schedule.len()
            ),
            Err(e) => {
                eprintln!("{f}: {e:#}");
                bad += 1;
            }
        }
    }
    if bad > 0 {
        eprintln!("panel-ddp: {bad} of {} files have mistakes", args.len());
        std::process::exit(1);
    }
    Ok(())
}

/// Rewrites a TOML or JSON config as YAML beside it (or at `--out`), the
/// Home Assistant token going to the secrets file (`secrets.yaml` beside
/// the new file, or `--secrets`), then loads the result and checks that it
/// draws what the original does.
fn cmd_migrate(args: &[String]) -> Result<()> {
    let mut old = None;
    let mut out = None;
    let mut secrets = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a file")?)),
            "--secrets" => secrets = Some(PathBuf::from(it.next().context("--secrets needs a file")?)),
            o if o.starts_with('-') => bail!("unknown option {o}"),
            f => old = Some(PathBuf::from(f)),
        }
    }
    let old = old.context("migrate needs the file to migrate")?;
    let out = out.unwrap_or_else(|| old.with_extension("yaml"));
    if out.exists() {
        bail!("{} exists; move it or name another with --out", out.display());
    }
    let dir = out.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let m = migrate::migrate(&old, dir)?;
    let mut yaml = m.yaml;
    let secrets = match secrets {
        Some(s) => {
            let shown = s.strip_prefix(dir).unwrap_or(&s);
            yaml = yaml.replacen("\ntarget:", &format!("\nsecrets: {}\ntarget:", shown.display()), 1);
            s
        }
        None => dir.join("secrets.yaml"),
    };
    if let Some(token) = &m.token {
        add_secret(&secrets, migrate::TOKEN_SECRET, token)?;
        eprintln!("panel-ddp: the Home Assistant token is in {} as {}", secrets.display(), migrate::TOKEN_SECRET);
    }
    std::fs::write(&out, yaml).with_context(|| format!("writing {}", out.display()))?;
    let original = legacy::load(&old)?;
    let migrated = model::load(&out).context("loading the migrated file")?;
    migrate::same_drawing(&original, &migrated)
        .with_context(|| format!("{} does not draw what {} does", out.display(), old.display()))?;
    eprintln!("panel-ddp: wrote {}; it draws every page as {} does", out.display(), old.display());
    Ok(())
}

/// Adds a secret to a secrets file, made readable by its owner alone; one
/// already there under the name must be the same.
fn add_secret(path: &std::path::Path, name: &str, value: &str) -> Result<()> {
    let mut values: indexmap::IndexMap<String, String> = match std::fs::read_to_string(path) {
        Ok(text) => serde_yaml_ng::from_str(&text).with_context(|| format!("parsing {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    match values.get(name) {
        Some(v) if v == value => return Ok(()),
        Some(_) => bail!("{} already has a different {name}", path.display()),
        None => {
            values.insert(name.to_string(), value.to_string());
        }
    }
    let text = serde_yaml_ng::to_string(&values)?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut f = opts.open(path).with_context(|| format!("writing {}", path.display()))?;
    std::io::Write::write_all(&mut f, text.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
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
        "render" => cmd_render(rest),
        "test" => cmd_test(rest),
        "spotify-login" => cmd_spotify_login(rest),
        "check" => cmd_check(rest),
        "migrate" => cmd_migrate(rest),
        "schema" => {
            print!("{}", format::schema());
            Ok(())
        }
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
