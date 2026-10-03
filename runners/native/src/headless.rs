//! Headless runs: N frames with virtual time, scripted input and (optionally)
//! hashes of everything the guest presents. Reproducible: the same module and
//! options give the same hashes on every runner (runners/web/headless.mjs).

use std::time::{Duration, Instant};

use crate::gfx::{Gfx, HEADLESS_SIZE};
use crate::host::{Stop, VirtualClock};
use crate::present::Present;
use crate::script::{Script, ScriptState};
use crate::session::Session;

pub struct Options {
    pub frames: u64,
    /// render on the GPU (offscreen) and write the last frame as PNG
    pub screenshot: Option<String>,
    /// also write the last 2D frame as the window would show it: `present` at `window` size
    pub screenshot_filtered: Option<String>,
    pub present: Present,
    pub window: (u32, u32),
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
    let gpu = opts.screenshot.is_some() || opts.screenshot_filtered.is_some();
    // a gasm:gl game can't use gasm:gfx: no wgpu for it (ANGLE renders below)
    let uses_gl = session.uses_gl();
    let gfx = match gpu && !uses_gl {
        true => Gfx::offscreen(HEADLESS_SIZE.0, HEADLESS_SIZE.1).unwrap_or_else(|e| {
            eprintln!("[gasm] no GPU for screenshots ({e}); gfx output will be blank");
            Gfx::null()
        }),
        false => Gfx::null(),
    };
    if let Some(t) = crate::host::static_title(&session.wasm) {
        eprintln!("[gasm] title: {t} (gasm.title)");
    }
    // gasm:gl games render with ANGLE offscreen for screenshots (the hashes don't change)
    let gl = match gpu && uses_gl {
        true => session.open_gl(None, HEADLESS_SIZE).map_err(|e| eprintln!("[gasm] no GL for screenshots ({e}); gl output will be blank")).ok(),
        false => None,
    };
    let mut game = match session.start(None, gfx, gl, true, opts.hash) {
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
        // between frames the host is reached through with_host (a gasm_run guest owns it)
        game.with_host(|host| {
            let rate = host.frame_rate;
            host.virtual_time_ms = Some(clock.at(i, rate));
            host.pads = [opts.script.pad(i), 0, 0, 0];
            host.text = Some(opts.script.text(i));
            let (w, h) = host.gfx.size();
            let mode = host.input_mode;
            host.input = opts.script.raw(i, &mut script_state, (w as f32, h as f32), mode);
            host.show_frame = gpu && i + 1 == opts.frames;
        });
        ran = i + 1;
        let result = game.frame();
        game.with_host(|h| { let show = h.show_frame; h.gl.end_frame(show) });
        if let Some(t) = game.with_host(|host| std::mem::take(&mut host.title_changed).then(|| host.title.clone())) {
            eprintln!("[gasm] title: {}", t.as_deref().unwrap_or("(default)"));
        }
        match result {
            Ok(()) => {}
            Err(Stop::Exit(code)) => {
                exit = Some(code);
                break;
            }
            Err(Stop::Trap(e)) => return Err(format!("frame {i}: {e}")),
        }
        if opts.realtime {
            let due = t0 + Duration::from_secs_f64((i + 1) as f64 / game.with_host(|h| h.frame_rate));
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
    }
    if exit.is_none() {
        game.exit();
    }
    if let Some(path) = &opts.screenshot_filtered {
        let h = game.host_mut();
        let img = match h.gfx.used {
            _ if h.gl.has_backend() => h.gl.read_frame(),
            true => h.gfx.read_offscreen(), // gfx guests render at drawable size: no filter
            false if h.width > 0 => {
                let rgba = std::mem::take(&mut h.rgba);
                h.gfx.present = opts.present;
                let img = h.gfx.render_video(&rgba, h.width as u32, h.height as u32, h.aspect, opts.window);
                h.rgba = rgba;
                img
            }
            false => None,
        };
        if let Some((w, hgt, rgba)) = img {
            write_png(path, w, hgt, &rgba)?;
            eprintln!("[gasm] wrote {path}");
        }
    }
    let h = game.host();
    if let Some(path) = &opts.screenshot {
        let (w, hgt, rgba) = match h.gfx.read_offscreen() {
            _ if h.gl.has_backend() => h.gl.read_frame().unwrap_or_default(),
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
