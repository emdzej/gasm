//! gasm-run — native runner for gasm ABI v0 games (wasmtime + wgpu + winit).

use gasm_host::{audio, gfx, host, net, storage};

use std::collections::{HashMap, HashSet};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gilrs::{Button, Gilrs};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use gfx::Gfx;
use host::{Game, Host, Stop};
use net::Net;
use storage::Storage;

const USAGE: &str = "\
usage: gasm-run <game.wasm|game.cwasm> [options]

options:
  --rom <path>             shorthand for --asset rom=<path>
  --asset <name>=<path>    expose a file to the guest as asset <name>
  --param <name>=<value>   launch parameter for the guest (repeatable)
  --allow-net              allow the guest to open network connections (gasm:net)
  --storage-dir <dir>      where the game's saves live (default: <data dir>/gasm/<game>;
                           headless runs use memory unless this is given)
  --storage-id <id>        storage namespace (default: the game file's name)
  --window <W>x<H>         initial window size in logical pixels (default 960x720)
  --mute                   no audio output
  --compile <out.cwasm>    AOT-compile the game to native code and exit
                           (then run the .cwasm instead of the .wasm)
  --headless <frames>      run N frames without window/audio, print hashes
  --screenshot <out.png>   (headless) write the last frame as PNG (renders gfx on the GPU)
  --input <script>         (headless) scripted input: FROM-TO:BTN+BTN,... (frame ranges,
                           buttons A B X Y L R SELECT START UP DOWN LEFT RIGHT)
  --realtime               (headless) pace frames at the guest's rate
  --no-hash                (headless) skip hashing, for benchmarking

keys: arrows = d-pad, X = A, Z = B, S = X, A = Y, Q/W = L/R,
      Enter = Start, Right Shift = Select, Esc = quit";

struct Args {
    wasm: String,
    assets: HashMap<String, String>,
    params: HashMap<String, String>,
    allow_net: bool,
    storage_dir: Option<String>,
    storage_id: Option<String>,
    window: (u32, u32),
    headless: Option<u64>,
    screenshot: Option<String>,
    compile: Option<String>,
    input: Vec<(u64, u64, u32)>,
    realtime: bool,
    mute: bool,
    no_hash: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut args = Args {
        wasm: String::new(),
        assets: HashMap::new(),
        params: HashMap::new(),
        allow_net: false,
        storage_dir: None,
        storage_id: None,
        window: (960, 720),
        headless: None,
        screenshot: None,
        compile: None,
        input: Vec::new(),
        realtime: false,
        mute: false,
        no_hash: false,
    };
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--rom" => {
                args.assets.insert("rom".into(), val("--rom")?);
            }
            "--asset" => {
                let v = val("--asset")?;
                let (k, p) = v.split_once('=').ok_or("--asset expects name=path")?;
                args.assets.insert(k.into(), p.into());
            }
            "--param" => {
                let v = val("--param")?;
                let (k, p) = v.split_once('=').ok_or("--param expects name=value")?;
                args.params.insert(k.into(), p.into());
            }
            "--allow-net" => args.allow_net = true,
            "--storage-dir" => args.storage_dir = Some(val("--storage-dir")?),
            "--storage-id" => args.storage_id = Some(val("--storage-id")?),
            "--window" => {
                let v = val("--window")?;
                let (w, h) = v.split_once('x').ok_or("--window expects WxH")?;
                args.window = (w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?);
            }
            "--headless" => {
                args.headless = Some(val("--headless")?.parse().map_err(|_| "--headless expects a number")?)
            }
            "--screenshot" => args.screenshot = Some(val("--screenshot")?),
            "--compile" => args.compile = Some(val("--compile")?),
            "--input" => args.input = parse_input_script(&val("--input")?)?,
            "--realtime" => args.realtime = true,
            "--mute" => args.mute = true,
            "--no-hash" => args.no_hash = true,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => args.wasm = s.into(),
        }
    }
    if args.wasm.is_empty() {
        return Err("missing <game.wasm>".into());
    }
    Ok(args)
}

/// Parse `FROM-TO:BTN+BTN,...` into (from, to_inclusive, mask) ranges.
fn parse_input_script(spec: &str) -> Result<Vec<(u64, u64, u32)>, String> {
    const NAMES: [&str; 12] = ["A", "B", "X", "Y", "L", "R", "SELECT", "START", "UP", "DOWN", "LEFT", "RIGHT"];
    spec.split(',')
        .map(|item| {
            let bad = || format!("bad --input item {item:?}");
            let (range, buttons) = item.split_once(':').ok_or_else(bad)?;
            let (from, to) = range.split_once('-').unwrap_or((range, range));
            let mut mask = 0;
            for b in buttons.split('+') {
                let bit = NAMES.iter().position(|n| n.eq_ignore_ascii_case(b)).ok_or_else(bad)?;
                mask |= 1 << bit;
            }
            Ok((from.parse().map_err(|_| bad())?, to.parse().map_err(|_| bad())?, mask))
        })
        .collect()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {e}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Loaded {
    wasm: Vec<u8>,
    assets: HashMap<String, Vec<u8>>,
}

fn run(args: Args) -> Result<i32, String> {
    let wasm = std::fs::read(&args.wasm).map_err(|e| format!("{}: {e}", args.wasm))?;
    if let Some(out) = &args.compile {
        let t0 = Instant::now();
        let native = host::precompile(&wasm).map_err(|e| format!("{e:?}"))?;
        std::fs::write(out, &native).map_err(|e| format!("{out}: {e}"))?;
        eprintln!("[gasm] compiled {} -> {out} ({} bytes) in {:.0} ms", args.wasm, native.len(), t0.elapsed().as_secs_f64() * 1000.0);
        return Ok(0);
    }
    let mut assets = HashMap::new();
    for (name, path) in &args.assets {
        assets.insert(name.clone(), std::fs::read(path).map_err(|e| format!("{path}: {e}"))?);
    }
    let loaded = Loaded { wasm, assets };
    match args.headless {
        Some(frames) => headless(&args, loaded, frames),
        None => windowed(args, loaded),
    }
}

fn open_storage(args: &Args) -> Result<Storage, Stop> {
    // Namespace = the game file's name (sumo.wasm / sumo.cwasm -> "sumo").
    let id = args.storage_id.clone().unwrap_or_else(|| {
        std::path::Path::new(&args.wasm).file_stem().map_or("game".into(), |s| s.to_string_lossy().into_owned())
    });
    if !storage::valid_key(&id) {
        return Err(Stop::Trap(format!("invalid storage id {id:?} (use [A-Za-z0-9._-])")));
    }
    let dir = match (&args.storage_dir, args.headless) {
        (Some(d), _) => Some(std::path::PathBuf::from(d)),
        (None, Some(_)) => None,
        (None, None) => storage::default_dir(&id),
    };
    let s = match dir {
        Some(d) => Storage::open(d).map_err(Stop::Trap)?,
        None => Storage::memory(),
    };
    eprintln!("[gasm] storage: {}", s.location());
    Ok(s)
}

fn load(args: &Args, loaded: &Loaded, audio: Option<audio::AudioSink>, gfx: Gfx) -> Result<Game, Stop> {
    let storage = open_storage(args)?;
    let mut host = Host::new(loaded.assets.clone(), args.params.clone(), audio, gfx, Net::new(args.allow_net), storage);
    if args.headless.is_some() {
        host.virtual_time_ms = Some(0.0);
    }
    host.hashing = args.headless.is_some() && !args.no_hash;
    let t0 = Instant::now();
    let game = Game::load(&loaded.wasm, host)?;
    eprintln!("[gasm] loaded {} in {:.0} ms", args.wasm, t0.elapsed().as_secs_f64() * 1000.0);
    Ok(game)
}

// ---- headless -----------------------------------------------------------------------

fn headless(args: &Args, loaded: Loaded, frames: u64) -> Result<i32, String> {
    // Render on the GPU only when a screenshot is wanted; otherwise a null backend.
    let gfx = match &args.screenshot {
        Some(_) => Gfx::offscreen(1280, 720).unwrap_or_else(|e| {
            eprintln!("[gasm] no GPU for screenshots ({e}); gfx output will be blank");
            Gfx::null()
        }),
        None => Gfx::null(),
    };
    let mut game = match load(args, &loaded, None, gfx) {
        Ok(g) => g,
        Err(Stop::Exit(code)) => return Ok(code),
        Err(Stop::Trap(e)) => return Err(e),
    };
    let t0 = Instant::now();
    let mut exit = None;
    let mut ran = 0;
    for i in 0..frames {
        let rate = game.host().frame_rate;
        let pad = args.input.iter().filter(|(f, t, _)| (*f..=*t).contains(&i)).fold(0, |m, (_, _, b)| m | b);
        let host = game.host_mut();
        host.virtual_time_ms = Some(i as f64 * 1000.0 / rate);
        host.pads = [pad, 0, 0, 0];
        host.show_frame = args.screenshot.is_some() && i + 1 == frames;
        ran = i + 1;
        match game.frame() {
            Ok(()) => {}
            Err(Stop::Exit(code)) => {
                exit = Some(code);
                break;
            }
            Err(Stop::Trap(e)) => return Err(format!("frame {i}: {e}")),
        }
        if args.realtime {
            let due = t0 + Duration::from_secs_f64((i + 1) as f64 / rate);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
    }
    if exit.is_none() {
        game.exit();
    }
    let secs = t0.elapsed().as_secs_f64();
    let h = game.host();
    println!("frames={ran} presented={} size={}x{}", h.frames_presented, h.width, h.height);
    println!("video_fnv32={:08x} audio_fnv32={:08x} audio_frames={}", h.video_hash.0, h.audio_hash.0, h.audio_frames);
    eprintln!(
        "[gasm] {:.1} guest frames/s ({:.1}x realtime at {:.2} Hz)",
        ran as f64 / secs,
        ran as f64 / secs / h.frame_rate,
        h.frame_rate
    );
    if let Some(path) = &args.screenshot {
        let (w, hgt, rgba) = match h.gfx.read_offscreen() {
            Some(img) if h.gfx.used => img,
            _ => (h.width as u32, h.height as u32, h.rgba.clone()),
        };
        if w > 0 {
            write_png(path, w, hgt, &rgba)?;
            eprintln!("[gasm] wrote {path}");
        }
    }
    if let Some(code) = exit {
        eprintln!("[gasm] guest exited with code {code}");
        return Ok(code);
    }
    Ok(0)
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

// ---- windowed ---------------------------------------------------------------------------

const KEYMAP: &[(KeyCode, u32)] = &[
    (KeyCode::KeyX, 1 << 0),
    (KeyCode::KeyZ, 1 << 1),
    (KeyCode::KeyS, 1 << 2),
    (KeyCode::KeyA, 1 << 3),
    (KeyCode::KeyQ, 1 << 4),
    (KeyCode::KeyW, 1 << 5),
    (KeyCode::ShiftRight, 1 << 6),
    (KeyCode::Enter, 1 << 7),
    (KeyCode::ArrowUp, 1 << 8),
    (KeyCode::ArrowDown, 1 << 9),
    (KeyCode::ArrowLeft, 1 << 10),
    (KeyCode::ArrowRight, 1 << 11),
];

fn gamepad_pads(gilrs: &mut Gilrs) -> [u32; 4] {
    const MAP: &[(Button, u32)] = &[
        (Button::East, 1 << 0),
        (Button::South, 1 << 1),
        (Button::North, 1 << 2),
        (Button::West, 1 << 3),
        (Button::LeftTrigger, 1 << 4),
        (Button::RightTrigger, 1 << 5),
        (Button::Select, 1 << 6),
        (Button::Start, 1 << 7),
        (Button::DPadUp, 1 << 8),
        (Button::DPadDown, 1 << 9),
        (Button::DPadLeft, 1 << 10),
        (Button::DPadRight, 1 << 11),
    ];
    while gilrs.next_event().is_some() {}
    let mut pads = [0u32; 4];
    for (i, (_, pad)) in gilrs.gamepads().take(4).enumerate() {
        pads[i] = MAP.iter().filter(|(b, _)| pad.is_pressed(*b)).fold(0, |m, (_, bit)| m | bit);
        let (x, y) = (pad.value(gilrs::Axis::LeftStickX), pad.value(gilrs::Axis::LeftStickY));
        if x < -0.5 { pads[i] |= 1 << 10 }
        if x > 0.5 { pads[i] |= 1 << 11 }
        if y > 0.5 { pads[i] |= 1 << 8 }
        if y < -0.5 { pads[i] |= 1 << 9 }
    }
    pads
}

struct App {
    args: Args,
    loaded: Loaded,
    window: Option<Arc<Window>>,
    game: Option<Game>,
    keys: HashSet<KeyCode>,
    gilrs: Option<Gilrs>,
    next: Instant,
    fps_t: Instant,
    fps_n: u32,
    result: Result<i32, String>,
}

impl App {
    fn stop(&mut self, el: &ActiveEventLoop, result: Result<i32, String>) {
        self.result = result;
        el.exit();
    }

    /// The player closed the window / pressed Esc: let the game save, then stop.
    fn quit(&mut self, el: &ActiveEventLoop) {
        if let Some(g) = &mut self.game {
            g.exit();
        }
        self.stop(el, Ok(0));
    }

    fn tick(&mut self, el: &ActiveEventLoop) {
        let Some(game) = &mut self.game else { return };
        let period = Duration::from_secs_f64(1.0 / game.host().frame_rate);
        let now = Instant::now();
        if now >= self.next {
            // Fixed timestep: catch up at most 4 frames, only the last one is shown.
            let behind = ((now - self.next).as_secs_f64() / period.as_secs_f64()) as u32 + 1;
            let steps = behind.min(4);
            for k in 0..steps {
                let mut pads = self.gilrs.as_mut().map(gamepad_pads).unwrap_or_default();
                pads[0] |= KEYMAP.iter().filter(|(k, _)| self.keys.contains(k)).fold(0, |m, (_, b)| m | b);
                let host = game.host_mut();
                host.pads = pads;
                host.show_frame = k + 1 == steps;
                host.gfx.used = false;
                match game.frame() {
                    Ok(()) => {}
                    Err(Stop::Exit(code)) => return self.stop(el, Ok(code)),
                    Err(Stop::Trap(e)) => return self.stop(el, Err(e)),
                }
                self.next += period;
                self.fps_n += 1;
            }
            if behind > 4 {
                self.next = Instant::now() + period;
            }
            // 2D guests: show the last video_present frame.
            let host = game.host_mut();
            if !host.gfx.used && host.width > 0 {
                let (w, h) = (host.width as u32, host.height as u32);
                let rgba = std::mem::take(&mut host.rgba);
                host.gfx.present_video(&rgba, w, h);
                host.rgba = rgba;
            }
        }
        if self.fps_t.elapsed() >= Duration::from_secs(1) {
            if let Some(w) = &self.window {
                w.set_title(&format!("gasm — {} — {} fps", self.args.wasm, self.fps_n));
            }
            self.fps_n = 0;
            self.fps_t = Instant::now();
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("gasm")
            .with_inner_size(LogicalSize::new(self.args.window.0, self.args.window.1));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => return self.stop(el, Err(format!("cannot create window: {e}"))),
        };
        let gfx = match Gfx::for_window(window.clone()) {
            Ok(g) => g,
            Err(e) => return self.stop(el, Err(format!("cannot initialise GPU: {e}"))),
        };
        let audio = if self.args.mute {
            None
        } else {
            match audio::AudioSink::open() {
                Ok(a) => {
                    eprintln!("[gasm] audio: {} Hz", a.device_rate());
                    Some(a)
                }
                Err(e) => {
                    eprintln!("[gasm] audio disabled: {e}");
                    None
                }
            }
        };
        match load(&self.args, &self.loaded, audio, gfx) {
            Ok(g) => self.game = Some(g),
            Err(Stop::Exit(code)) => return self.stop(el, Ok(code)),
            Err(Stop::Trap(e)) => return self.stop(el, Err(e)),
        }
        self.window = Some(window);
        self.next = Instant::now();
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.game {
                    g.host_mut().gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape {
                        return self.quit(el);
                    }
                    match event.state {
                        ElementState::Pressed => self.keys.insert(code),
                        ElementState::Released => self.keys.remove(&code),
                    };
                }
            }
            WindowEvent::RedrawRequested => self.tick(el),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.tick(el);
    }
}

fn windowed(args: Args, loaded: Loaded) -> Result<i32, String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App {
        args,
        loaded,
        window: None,
        game: None,
        keys: HashSet::new(),
        gilrs: Gilrs::new().ok(),
        next: Instant::now(),
        fps_t: Instant::now(),
        fps_n: 0,
        result: Ok(0),
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.result
}
