//! Headless runs: N frames with virtual time, scripted input and (optionally)
//! hashes of everything the guest presents. Reproducible: the same module and
//! options give the same hashes on every runner (runners/web/headless.mjs).

use std::time::{Duration, Instant};

use crate::gfx::{Gfx, HEADLESS_SIZE};
use crate::host::{Stop, VirtualClock};
use crate::script::{Script, ScriptState};
use crate::session::Session;

pub struct Options {
    pub frames: u64,
    /// render on the GPU (offscreen) and write the last frame as PNG
    pub screenshot: Option<String>,
    pub script: Script,
    /// pace frames at the guest's rate
    pub realtime: bool,
    pub hash: bool,
}

/// What a run produced (`print` writes the hash lines scripts compare).
pub struct Report {
    pub frames: u64,
    pub presented: u64,
    pub size: (usize, usize),
    pub video_hash: u32,
    pub audio_hash: u32,
    pub audio_frames: u64,
    pub frame_rate: f64,
    pub seconds: f64,
    /// the guest called proc_exit(code)
    pub exit: Option<i32>,
}

impl Report {
    /// The hash lines (stdout): a contract between runners and scripts.
    pub fn print(&self) {
        println!("frames={} presented={} size={}x{}", self.frames, self.presented, self.size.0, self.size.1);
        println!("video_fnv32={:08x} audio_fnv32={:08x} audio_frames={}", self.video_hash, self.audio_hash, self.audio_frames);
    }
}

pub fn run(session: Session, opts: &Options) -> Result<Report, String> {
    // Render on the GPU only when a screenshot is wanted; otherwise a null backend.
    let gfx = match &opts.screenshot {
        Some(_) => Gfx::offscreen(HEADLESS_SIZE.0, HEADLESS_SIZE.1).unwrap_or_else(|e| {
            eprintln!("[gasm] no GPU for screenshots ({e}); gfx output will be blank");
            Gfx::null()
        }),
        None => Gfx::null(),
    };
    let mut game = match session.start(None, gfx, true, opts.hash) {
        Ok(g) => g,
        Err(Stop::Exit(code)) => {
            return Ok(Report { frames: 0, presented: 0, size: (0, 0), video_hash: 0x811c_9dc5, audio_hash: 0x811c_9dc5, audio_frames: 0, frame_rate: 60.0, seconds: 0.0, exit: Some(code) });
        }
        Err(Stop::Trap(e)) => return Err(e),
    };
    let t0 = Instant::now();
    let mut exit = None;
    let mut ran = 0;
    let mut clock = VirtualClock::default();
    let mut script_state = ScriptState::default();
    for i in 0..opts.frames {
        let host = game.host_mut();
        let rate = host.frame_rate;
        host.virtual_time_ms = Some(clock.at(i, rate));
        host.pads = [opts.script.pad(i), 0, 0, 0];
        host.text = Some(opts.script.text(i));
        let (w, h) = host.gfx.size();
        let mode = host.input_mode;
        host.input = opts.script.raw(i, &mut script_state, (w as f32, h as f32), mode);
        host.show_frame = opts.screenshot.is_some() && i + 1 == opts.frames;
        ran = i + 1;
        match game.frame() {
            Ok(()) => {}
            Err(Stop::Exit(code)) => {
                exit = Some(code);
                break;
            }
            Err(Stop::Trap(e)) => return Err(format!("frame {i}: {e}")),
        }
        if opts.realtime {
            let due = t0 + Duration::from_secs_f64((i + 1) as f64 / game.host().frame_rate);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
    }
    if exit.is_none() {
        game.exit();
    }
    let h = game.host();
    if let Some(path) = &opts.screenshot {
        let (w, hgt, rgba) = match h.gfx.read_offscreen() {
            Some(img) if h.gfx.used => img,
            _ => (h.width as u32, h.height as u32, h.rgba.clone()),
        };
        if w > 0 {
            write_png(path, w, hgt, &rgba)?;
            eprintln!("[gasm] wrote {path}");
        }
    }
    Ok(Report {
        frames: ran,
        presented: h.frames_presented,
        size: (h.width, h.height),
        video_hash: h.video_hash.0,
        audio_hash: h.audio_hash.0,
        audio_frames: h.audio_frames,
        frame_rate: h.frame_rate,
        seconds: t0.elapsed().as_secs_f64(),
        exit,
    })
}

pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}
