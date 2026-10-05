//! gasm-run — native runner for gasm ABI v0 games (wasmtime + wgpu + winit).
//! Argument parsing only: the runners are in the gasm-host library.

use gasm_host::host::{self, LoadOptions};
use gasm_host::session::Session;
use gasm_host::present::{Filter, Present};
use gasm_host::{assets, headless, script, storage};

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use storage::Storage;

const USAGE: &str = "\
usage: gasm-run <game.wasm> [options]

options:
  --rom <path>             shorthand for --asset rom=<path>
  --asset <name>=<path>    expose a file to the guest as asset <name> (read on demand, not preloaded)
  --asset-dir [<prefix>=]<dir>
                           expose every file under <dir> (repeatable); names are relative paths
                           like ART/ART.CAR (or <prefix>/ART/ART.CAR), looked up case-insensitively;
                           symlinks and hidden files are skipped; --asset wins over folder entries
  --watch-asset <name>=<path>
                           like --asset, but re-read whenever the file changes (rewritten or replaced
                           by a rename; it may also appear later): the guest sees the new data from the
                           next frame, with a new gasm.asset_version (repeatable)
  --param <name>=<value>   launch parameter for the guest (repeatable)
  --allow-net[=<hosts>]    allow the guest to open network connections (gasm:net) and make HTTP
                           requests (gasm:fetch); with a comma-separated list only to those hosts
                           (api.example.org, *.example.org for its subdomains)
  --app-id <text>          who the game is in its HTTP requests: gasm:fetch sends
                           User-Agent: <text> gasm-run/<version>, e.g. --app-id 'mygame/1.0 (+https://mygame.example)'
  --fetch-record <dir>     store every gasm:fetch response in <dir> (with --allow-net)
  --fetch-replay <dir>     answer gasm:fetch requests from <dir> only (never the network): each
                           completes at the next frame, so headless runs stay reproducible
  --storage-dir <dir>      where the game's saves live (default: <data dir>/gasm/<game>;
                           headless runs use memory unless this is given)
  --storage-id <id>        storage namespace (default: the game file's name)
  --window <W>x<H>         initial window size in logical pixels (default 960x720)
  --filter <name>          how 2D frames are scaled up: sharp (default; even pixels at any size),
                           nearest (plain pixel doubling), xbr (smooth edges for pixel art),
                           fsr (AMD FSR 1, for rendered or dithered content), crt (scanlines)
  --integer-scale          scale 2D frames by whole multiples only (black border around)
  --keymap <file>          keyboard layout (default: <data dir>/gasm/keymap.txt if it exists,
                           else the built-in two-player layout below)
  --print-keymap           print the active keyboard layout (a starting point for --keymap) and exit
  --mute                   no audio output
  --no-splash              start the game without the gasm splash screen (window only)
  --compile <out.cwasm>    AOT-compile the game to native code and exit
                           (then run the .cwasm with --allow-precompiled)
  --allow-precompiled      accept a .cwasm: native code, so only files you compiled yourself
  --call-timeout <secs>    trap a guest call (init, a frame) that runs longer (default 30, 0 = never)
  --memory-limit <MiB>     trap when the guest's memory grows past this (default 1024, 0 = 4 GiB)
  --no-stack-switching     call gasm_frame even if the game exports gasm_run (its Asyncify path)
  --gl-lib <dir>           where ANGLE's libEGL/libGLESv2 are, for gasm:gl games (default: next
                           to gasm-run, ../Frameworks in a .app, or $GASM_ANGLE_DIR)
  --gl-software            gasm:gl on SwiftShader (software Vulkan) instead of the GPU
  --window-screenshot <frames>:<out.png>
                           (window) write that frame as shown, then quit (gasm:gl games; tests)
  --headless <frames>      run N frames without window/audio, print hashes
  --screenshot <out.png>   (headless) write the last frame as PNG (renders gfx on the GPU)
  --screenshot-filtered <out.png>
                           (headless) write the last frame as the window shows it: --filter and
                           --integer-scale at --window size, in pixels
  --input <script>         (headless) scripted input, FRAMES:ACTION,... with FRAMES = N or FROM-TO:
                           A+B+START (pad 1: A B X Y L R SELECT START UP DOWN LEFT RIGHT),
                           \"text\" (text_input; escapes \\n \\b), KEY(ShiftLeft+ArrowLeft) (raw keys),
                           PTR(x,y[,L+R+M]) (pointer, drawable px), MOVE(dx,dy), WHEEL(x,y),
                           GP0(B0+B9+A1=0.5) (gamepad slot 0-3: buttons, axes)
  --realtime               (headless) pace frames at the guest's rate
  --no-hash                (headless) skip hashing, for benchmarking

default keys (change with --keymap; one binding per line: <pad 1-4> <button> <key code>...):
  pad 1: arrows = d-pad, X = A, Z = B, S = X, A = Y, Q/W = L/R,
         Enter = Start, Right Shift = Select
  pad 2: I/J/K/L = d-pad, . = A, , = B, M = X, N = Y, U/O = L/R,
         Right Ctrl or keypad Enter = Start, Backspace = Select
  Keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected;
  gamepads take pads in connection order.
Esc = quit";

struct Args {
    wasm: String,
    assets: HashMap<String, String>,
    asset_dirs: Vec<(Option<String>, String)>,
    watch_assets: Vec<(String, String)>,
    params: HashMap<String, String>,
    allow_net: bool,
    no_splash: bool,
    allow_hosts: Vec<String>,
    fetch: gasm_host::fetch::FetchMode,
    storage_dir: Option<String>,
    storage_id: Option<String>,
    app_id: Option<String>,
    window: (u32, u32),
    present: Present,
    keymap: Option<String>,
    print_keymap: bool,
    headless: Option<u64>,
    screenshot: Option<String>,
    screenshot_filtered: Option<String>,
    compile: Option<String>,
    /// headless scripted input
    script: script::Script,
    realtime: bool,
    mute: bool,
    no_hash: bool,
    allow_precompiled: bool,
    call_timeout: Option<Duration>,
    memory_limit: Option<usize>,
    stack_switching: bool,
    gl_lib: Option<String>,
    gl_software: bool,
    window_screenshot: Option<(u64, String)>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut args = Args {
        wasm: String::new(),
        assets: HashMap::new(),
        watch_assets: Vec::new(),
        asset_dirs: Vec::new(),
        params: HashMap::new(),
        allow_net: false,
        no_splash: false,
        allow_hosts: Vec::new(),
        fetch: Default::default(),
        storage_dir: None,
        storage_id: None,
        app_id: None,
        window: (960, 720),
        present: Present::default(),
        keymap: None,
        print_keymap: false,
        headless: None,
        screenshot: None,
        screenshot_filtered: None,
        compile: None,
        script: script::Script::default(),
        realtime: false,
        mute: false,
        no_hash: false,
        allow_precompiled: false,
        call_timeout: Some(Duration::from_secs(30)),
        memory_limit: Some(gasm_host::host::DEFAULT_MEMORY_LIMIT),
        stack_switching: true,
        gl_lib: None,
        gl_software: false,
        window_screenshot: None,
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
            "--watch-asset" => {
                let v = val("--watch-asset")?;
                let (k, p) = v.split_once('=').ok_or("--watch-asset expects name=path")?;
                args.watch_assets.push((k.into(), p.into()));
            }
            "--asset-dir" => {
                let v = val("--asset-dir")?;
                // "prefix=dir" if the part before '=' is a plain name; otherwise the whole value is a dir
                let dir = match v.split_once('=') {
                    Some((p, d)) if !p.is_empty() && !p.contains(['/', '\\']) => (Some(p.to_owned()), d.to_owned()),
                    _ => (None, v),
                };
                args.asset_dirs.push(dir);
            }
            "--param" => {
                let v = val("--param")?;
                let (k, p) = v.split_once('=').ok_or("--param expects name=value")?;
                args.params.insert(k.into(), p.into());
            }
            "--allow-net" => args.allow_net = true,
            "--no-splash" => args.no_splash = true,
            a if a.starts_with("--allow-net=") => {
                args.allow_net = true;
                args.allow_hosts = a["--allow-net=".len()..].split(',').map(str::trim).filter(|h| !h.is_empty()).map(String::from).collect();
                if args.allow_hosts.is_empty() {
                    return Err("--allow-net= expects host names".into());
                }
            }
            "--fetch-record" => args.fetch = gasm_host::fetch::FetchMode::Record(val("--fetch-record")?.into()),
            "--fetch-replay" => args.fetch = gasm_host::fetch::FetchMode::Replay(val("--fetch-replay")?.into()),
            "--storage-dir" => args.storage_dir = Some(val("--storage-dir")?),
            "--storage-id" => args.storage_id = Some(val("--storage-id")?),
            "--app-id" => {
                let id = val("--app-id")?;
                if !gasm_host::fetch::valid_app_id(&id) {
                    return Err("--app-id expects 1 to 256 printable ASCII characters".into());
                }
                args.app_id = Some(id);
            }
            "--window" => {
                let v = val("--window")?;
                let (w, h) = v.split_once('x').ok_or("--window expects WxH")?;
                args.window = (w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?);
            }
            "--filter" => args.present.filter = Filter::parse(&val("--filter")?)?,
            "--integer-scale" => args.present.integer_scale = true,
            "--keymap" => args.keymap = Some(val("--keymap")?),
            "--print-keymap" => args.print_keymap = true,
            "--headless" => {
                args.headless = Some(val("--headless")?.parse().map_err(|_| "--headless expects a number")?)
            }
            "--screenshot" => args.screenshot = Some(val("--screenshot")?),
            "--window-screenshot" => {
                let v = val("--window-screenshot")?;
                let (n, path) = v.split_once(':').ok_or("--window-screenshot expects <frames>:<out.png>")?;
                let n: u64 = n.parse().map_err(|_| "--window-screenshot expects <frames>:<out.png>")?;
                args.window_screenshot = Some((n.max(1), path.to_owned()));
            }
            "--screenshot-filtered" => args.screenshot_filtered = Some(val("--screenshot-filtered")?),
            "--compile" => args.compile = Some(val("--compile")?),
            "--input" => args.script = script::Script::parse(&val("--input")?)?,
            "--realtime" => args.realtime = true,
            "--mute" => args.mute = true,
            "--no-hash" => args.no_hash = true,
            "--allow-precompiled" => args.allow_precompiled = true,
            "--no-stack-switching" => args.stack_switching = false,
            "--gl-lib" => args.gl_lib = Some(val("--gl-lib")?),
            "--gl-software" => args.gl_software = true,
            "--call-timeout" => {
                let secs: f64 = val("--call-timeout")?.parse().map_err(|_| "--call-timeout expects seconds")?;
                if !(secs >= 0.0 && secs.is_finite()) {
                    return Err("--call-timeout expects seconds".into());
                }
                args.call_timeout = (secs > 0.0).then(|| Duration::from_secs_f64(secs));
            }
            "--memory-limit" => {
                let mib: usize = val("--memory-limit")?.parse().map_err(|_| "--memory-limit expects MiB")?;
                args.memory_limit = (mib > 0).then_some(mib << 20);
            }
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => args.wasm = s.into(),
        }
    }
    if args.wasm.is_empty() && !args.print_keymap {
        return Err("missing <game.wasm>".into());
    }
    Ok(args)
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
        // exit codes outside 0-255 can't be passed on: report them as a failure
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Open (not read) every asset: explicit files first, so they win over folder entries.
/// Also returns the --watch-asset watches.
fn open_assets(args: &Args) -> Result<(assets::Assets, Vec<assets::AssetWatch>), String> {
    let mut a = assets::Assets::new();
    for (name, path) in &args.assets {
        a.insert_file(name, std::path::Path::new(path))?;
    }
    let mut watches = Vec::new();
    for (name, path) in &args.watch_assets {
        let path = std::path::Path::new(path);
        // the file's state before it's opened: a change in between is picked up later
        watches.push(assets::AssetWatch::new(name, path));
        // a watched file may not exist yet: the watch adds it when it appears
        if path.exists() {
            a.insert_file(name, path)?;
        }
    }
    for (prefix, dir) in &args.asset_dirs {
        let n = a.add_dir(prefix.as_deref(), std::path::Path::new(dir))?;
        eprintln!("[gasm] assets: {n} files from {dir}{}", prefix.as_ref().map_or(String::new(), |p| format!(" as {p}/")));
    }
    a.finish();
    Ok((a, watches))
}

/// --keymap FILE, else <data dir>/gasm/keymap.txt if present, else the default.
#[cfg(feature = "window")]
fn load_keymap(args: &Args) -> Result<(gasm_host::keymap::Keymap, String, String), String> {
    use gasm_host::keymap;
    let (text, source) = match &args.keymap {
        Some(p) => (std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?, p.clone()),
        None => match storage::data_root().map(|r| r.join("keymap.txt")).filter(|p| p.is_file()) {
            Some(p) => (std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?, p.display().to_string()),
            None => (keymap::DEFAULT_KEYMAP.to_owned(), "built-in".to_owned()),
        },
    };
    let map = keymap::parse(&text).map_err(|e| format!("keymap {source}:\n{e}"))?;
    Ok((map, text, source))
}

fn open_storage(args: &Args) -> Result<Storage, String> {
    // Namespace = the game file's name (sumo.wasm / sumo.cwasm -> "sumo").
    let id = args.storage_id.clone().unwrap_or_else(|| {
        std::path::Path::new(&args.wasm).file_stem().map_or("game".into(), |s| s.to_string_lossy().into_owned())
    });
    if !storage::valid_key(&id) {
        return Err(format!("invalid storage id {id:?} (use [A-Za-z0-9._-])"));
    }
    let dir = match (&args.storage_dir, args.headless) {
        (Some(d), _) => Some(std::path::PathBuf::from(d)),
        (None, Some(_)) => None,
        (None, None) => storage::default_dir(&id),
    };
    let s = match dir {
        Some(d) => Storage::open(d)?,
        None => Storage::memory(),
    };
    eprintln!("[gasm] storage: {}", s.location());
    Ok(s)
}

fn run(args: Args) -> Result<i32, String> {
    if args.print_keymap {
        #[cfg(feature = "window")]
        {
            print!("{}", load_keymap(&args)?.1);
            return Ok(0);
        }
        #[cfg(not(feature = "window"))]
        return Err("--print-keymap needs the window feature".into());
    }
    let wasm = std::fs::read(&args.wasm).map_err(|e| format!("{}: {e}", args.wasm))?;
    if let Some(out) = &args.compile {
        let t0 = Instant::now();
        let native = host::precompile(&wasm).map_err(|e| format!("{e:?}"))?;
        std::fs::write(out, &native).map_err(|e| format!("{out}: {e}"))?;
        eprintln!("[gasm] compiled {} -> {out} ({} bytes) in {:.0} ms", args.wasm, native.len(), t0.elapsed().as_secs_f64() * 1000.0);
        return Ok(0);
    }
    // fail fast on bad paths, before opening a window
    let (assets, watch_assets) = open_assets(&args)?;
    let session = Session {
        name: args.wasm.clone(),
        wasm,
        assets,
        params: args.params.clone(),
        allow_net: args.allow_net,
        allow_hosts: args.allow_hosts.clone(),
        fetch: args.fetch.clone(),
        app_id: args.app_id.clone(),
        storage: open_storage(&args)?,
        load: LoadOptions {
            allow_precompiled: args.allow_precompiled,
            call_timeout: args.call_timeout,
            stack_switching: args.stack_switching,
            memory_limit: args.memory_limit,
        },
        gl_lib: args.gl_lib.as_ref().map(std::path::PathBuf::from),
        gl_software: args.gl_software,
        watch_assets,
    };
    match args.headless {
        Some(frames) => {
            let opts = headless::Options {
                frames,
                screenshot: args.screenshot.clone(),
                screenshot_filtered: args.screenshot_filtered.clone(),
                present: args.present,
                window: args.window,
                script: args.script.clone(),
                realtime: args.realtime,
                hash: !args.no_hash,
            };
            let r = headless::run(session, &opts)?;
            if r.frames == 0 && r.exit.is_some() {
                return Ok(r.exit.unwrap_or(0)); // exited during init
            }
            r.print();
            eprintln!(
                "[gasm] {:.1} guest frames/s ({:.1}x realtime at {:.2} Hz)",
                r.frames as f64 / r.seconds,
                r.frames as f64 / r.seconds / r.frame_rate,
                r.frame_rate
            );
            if let Some(code) = r.exit {
                eprintln!("[gasm] guest exited with code {code}");
                return Ok(code);
            }
            Ok(0)
        }
        #[cfg(feature = "window")]
        None => {
            let (keymap, _, source) = load_keymap(&args)?;
            eprintln!("[gasm] keyboard layout: {source}");
            gasm_host::window::run(session, gasm_host::window::Options { size: args.window, keymap, mute: args.mute, present: args.present, screenshot: args.window_screenshot.clone(), splash: !args.no_splash && args.window_screenshot.is_none() })
        }
        #[cfg(not(feature = "window"))]
        None => Err("this gasm-run was built without the window feature: use --headless".into()),
    }
}
