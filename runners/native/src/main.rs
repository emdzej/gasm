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
  --mods <dir>             the game's mods: the folder's resource packs (*.pck, *.zip), read on
                           demand as assets mods/<name> in name order (Godot: Gasm.get_mods())
  --no-mods                no mods, even with --mods (for launchers that always pass it)
  --save-dir <dir>         where files the game saves for the player go (gasm:files; default:
                           Pictures/<game>/ for images, Downloads/<game>/ otherwise; headless:
                           nowhere unless given)
  --no-save                refuse every file the game wants to save for the player
  --manifest <file>        the game's capabilities manifest (JSON; else asset gasm.manifest, else the
                           module's gasm.manifest section): what it requires, the hosts it reaches
                           and whether it saves files, asked about once before it starts
  --icon <png>             the window's icon (Windows, Linux; else the manifest's icon asset; macOS
                           uses the app bundle's)
  --app-class <name>       (Linux) the window's X11 class / Wayland app id, to match a .desktop entry
  --info                   print the game's manifest and imports, whether this runner has them, and exit
  --no-ask                 (window) don't ask the player: refuse network hosts and saves the
                           options above didn't allow (headless runs never ask)
  --forget-consent <game|all>
                           forget the answers remembered for <game> (its id: the file name or
                           --storage-id), or for every game, and exit
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
  --copy-key <code>        (window) copy the game's frame to the clipboard on this key (default F2;
                           a KeyboardEvent.code name, or none); the key also reaches the game
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
  --gl-stats               count gasm:gl calls and print them (a frame, by name) when the game ends
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
    save_dir: Option<String>,
    mods: Option<String>,
    no_mods: bool,
    no_save: bool,
    no_ask: bool,
    manifest: Option<String>,
    info: bool,
    icon: Option<String>,
    app_class: Option<String>,
    forget_consent: Option<String>,
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
    copy_key: String,
    no_hash: bool,
    allow_precompiled: bool,
    call_timeout: Option<Duration>,
    memory_limit: Option<usize>,
    stack_switching: bool,
    gl_lib: Option<String>,
    gl_software: bool,
    gl_stats: bool,
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
        save_dir: None,
        mods: None,
        no_mods: false,
        no_save: false,
        no_ask: false,
        manifest: None,
        info: false,
        icon: None,
        app_class: None,
        forget_consent: None,
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
        copy_key: "F2".into(),
        no_hash: false,
        allow_precompiled: false,
        call_timeout: Some(Duration::from_secs(30)),
        memory_limit: Some(gasm_host::host::DEFAULT_MEMORY_LIMIT),
        stack_switching: true,
        gl_lib: None,
        gl_software: false,
        gl_stats: false,
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
            "--save-dir" => args.save_dir = Some(val("--save-dir")?),
            "--mods" => args.mods = Some(val("--mods")?),
            "--no-mods" => args.no_mods = true,
            "--no-save" => args.no_save = true,
            "--no-ask" => args.no_ask = true,
            "--manifest" => args.manifest = Some(val("--manifest")?),
            "--info" => args.info = true,
            "--icon" => args.icon = Some(val("--icon")?),
            "--app-class" => args.app_class = Some(val("--app-class")?),
            "--forget-consent" => args.forget_consent = Some(val("--forget-consent")?),
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
            "--copy-key" => args.copy_key = val("--copy-key")?,
            "--no-hash" => args.no_hash = true,
            "--allow-precompiled" => args.allow_precompiled = true,
            "--no-stack-switching" => args.stack_switching = false,
            "--gl-lib" => args.gl_lib = Some(val("--gl-lib")?),
            "--gl-software" => args.gl_software = true,
            "--gl-stats" => args.gl_stats = true,
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
    if args.wasm.is_empty() && !args.print_keymap && args.forget_consent.is_none() {
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
/// Mods whose manifests (`<stem>.json` next to the pack) ask for hosts: (file, hosts).
type ModRequests = Vec<(String, Vec<String>)>;

fn open_assets(args: &Args) -> Result<(assets::Assets, Vec<assets::AssetWatch>, ModRequests), String> {
    let mut requests = Vec::new();
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
    // --mods: resource packs as mods/<name> (a missing folder is just no mods)
    match (&args.mods, args.no_mods) {
        (Some(_), true) => eprintln!("[gasm] mods: off (--no-mods)"),
        (Some(dir), false) if std::path::Path::new(dir).is_dir() => {
            let (mounted, refused) = a.add_mods(std::path::Path::new(dir))?;
            eprintln!("[gasm] mods: {} from {dir}{}", mounted.len(), if mounted.is_empty() { String::new() } else { format!(": {}", mounted.join(", ")) });
            for (name, why) in refused {
                eprintln!("[gasm] mods: refused {name} ({why})");
            }
            // each mod's manifest, if it has one: what it asks for (a broken one refuses the mod)
            for name in mounted {
                let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
                let path = std::path::Path::new(dir).join(format!("{stem}.json"));
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                match gasm_host::manifest::Manifest::parse(&text) {
                    Ok(m) if m.hosts.is_empty() => {}
                    Ok(m) => requests.push((name, m.hosts)),
                    Err(e) => {
                        let why = format!("its manifest {stem}.json: {e}");
                        eprintln!("[gasm] mods: refused {name} ({why})");
                        a.refuse_mod(&name, &why);
                    }
                }
            }
        }
        (Some(dir), false) => eprintln!("[gasm] mods: none ({dir} isn't a folder)"),
        _ => {}
    }
    a.finish();
    Ok((a, watches, requests))
}

/// The game's manifest: --manifest <file>, else asset `gasm.manifest`, else the module's
/// `gasm.manifest` section. One that's present but invalid refuses the game.
fn load_manifest(args: &Args, wasm: &[u8]) -> Result<Option<gasm_host::manifest::Manifest>, String> {
    use gasm_host::manifest::{self, Manifest};
    let asset = args.assets.iter().find(|(n, _)| n.as_str() == manifest::SECTION).map(|(_, p)| p.clone());
    let (text, from) = match args.manifest.clone().or(asset) {
        Some(f) => (std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?, f),
        None => match manifest::from_module(wasm) {
            Some(t) => (t?, format!("{} (its gasm.manifest section)", args.wasm)),
            None => return Ok(None),
        },
    };
    let mut m = Manifest::parse(&text).map_err(|e| format!("the game's manifest {from}: {e}"))?;
    // the id names the game's saves: only the launcher may choose it (a game claiming another's
    // id would read its saves), so an embedded one doesn't count
    if from.ends_with("(its gasm.manifest section)") {
        if let Some(id) = m.id.take() {
            eprintln!("[gasm] manifest: id {id:?} ignored: only a manifest the launcher gives (--manifest, asset gasm.manifest) names the game's saves");
        }
    }
    eprintln!(
        "[gasm] manifest: {}{}{}{}",
        m.name.as_deref().unwrap_or("(no name)"),
        if m.requires.is_empty() { String::new() } else { format!(", requires {}", m.requires.join(" ")) },
        if m.hosts.is_empty() { String::new() } else { format!(", hosts {}", m.hosts.join(" ")) },
        if m.files { ", saves files" } else { "" }
    );
    Ok(Some(m))
}

/// `--info`: the game's manifest and imports, and whether this runner has them.
fn info(args: &Args, wasm: &[u8]) -> Result<i32, String> {
    let provided = host::provided();
    let manifest = load_manifest(args, wasm)?;
    println!("{}", args.wasm);
    match &manifest {
        Some(m) => {
            println!("  manifest: {}", m.name.as_deref().unwrap_or("(no name)"));
            println!("    requires: {}", if m.requires.is_empty() { "-".into() } else { m.requires.join(" ") });
            println!("    hosts:    {}", if m.hosts.is_empty() { "-".into() } else { m.hosts.join(" ") });
            println!("    files:    {}", if m.files { "saves files for the player" } else { "-" });
        }
        None => println!("  manifest: none"),
    }
    let mut missing = manifest.as_ref().map(|m| m.missing(&provided)).unwrap_or_default();
    match wasmtime::Module::new(host::engine(), wasm) {
        Ok(module) => {
            let mut modules: Vec<String> = module.imports().map(|i| i.module().to_owned()).collect();
            modules.sort();
            modules.dedup();
            println!("  imports:");
            for m in modules {
                let n = module.imports().filter(|i| i.module() == m).count();
                let unknown: Vec<String> = module.imports().filter(|i| i.module() == m && !provided.contains(&format!("{}.{}", m, i.name()))).map(|i| i.name().to_owned()).collect();
                println!("    {m:<24} {n} function{}{}", if n == 1 { "" } else { "s" }, if unknown.is_empty() { String::new() } else { format!(" ({} not here: {})", unknown.len(), unknown.iter().take(4).cloned().collect::<Vec<_>>().join(", ")) });
            }
        }
        Err(e) => println!("  imports: (not a module this runner compiles: {e})"),
    }
    missing.dedup();
    if missing.is_empty() {
        println!("  this runner can run it (gasm-run {})", env!("CARGO_PKG_VERSION"));
        Ok(0)
    } else {
        println!("  this runner lacks {} (gasm-run {})", missing.join(", "), env!("CARGO_PKG_VERSION"));
        Ok(1)
    }
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

/// The game's id (its saves, remembered answers, default folders): --storage-id, else the
/// manifest's "id", else a Godot pack's file name (--asset game.pck=mygame.pck: one engine
/// module serves every Godot game), else the game file's name (sumo.wasm -> "sumo").
fn game_id(args: &Args, manifest: Option<&gasm_host::manifest::Manifest>) -> String {
    let stem = |p: &str| std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().into_owned());
    args.storage_id
        .clone()
        .or_else(|| manifest.and_then(|m| m.id.clone()))
        .or_else(|| args.assets.iter().find(|(n, _)| n.as_str() == "game.pck").and_then(|(_, p)| stem(p)).filter(|s| storage::valid_key(s)))
        .or_else(|| stem(&args.wasm))
        .unwrap_or_else(|| "game".into())
}

fn open_storage(args: &Args, id: &str) -> Result<Storage, String> {
    let id = id.to_owned();
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
    if let Some(game) = &args.forget_consent {
        eprintln!("[gasm] consent: {}", gasm_host::consent::forget(game)?);
        return Ok(0);
    }
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
    if args.info {
        return info(&args, &wasm);
    }
    let manifest = load_manifest(&args, &wasm)?;
    // fail fast on bad paths, before opening a window
    let (assets, watch_assets, mod_requests) = open_assets(&args)?;
    let id = game_id(&args, manifest.as_ref());
    let consent = (args.headless.is_none() && !args.no_ask).then(|| gasm_host::consent::Store::open(&id).shared());
    if let Some(c) = &consent {
        // asked before the game starts: the manifest's hosts and saves, and mods that want hosts
        let allowed = gasm_host::net::NetPolicy::new(args.allow_net, args.allow_hosts.clone());
        let mut c = c.lock().unwrap();
        if let Some(m) = &manifest {
            let hosts: Vec<String> = m.hosts.iter().filter(|h| !(allowed.allowed && allowed.permits(h))).cloned().collect();
            c.ask_up_front(&hosts, m.files && args.save_dir.is_none() && !args.no_save);
        }
        for (file, hosts) in &mod_requests {
            if !(allowed.allowed && hosts.iter().all(|h| allowed.permits(h))) {
                _ = c.check_mod(file, hosts);
            }
        }
    }
    let session = Session {
        name: args.wasm.clone(),
        wasm,
        assets,
        params: args.params.clone(),
        allow_net: args.allow_net,
        allow_hosts: args.allow_hosts.clone(),
        fetch: args.fetch.clone(),
        app_id: args.app_id.clone(),
        consent,
        manifest,
        mod_requests,
        save: match (&args.save_dir, args.no_save, args.headless) {
            (_, true, _) => gasm_host::files::SaveTarget::Off,
            (Some(d), _, _) => gasm_host::files::SaveTarget::Dir(d.into()),
            (None, _, Some(_)) => gasm_host::files::SaveTarget::Discard,
            (None, _, None) => gasm_host::files::SaveTarget::Defaults { game: id.clone() },
        },
        storage: open_storage(&args, &id)?,
        load: LoadOptions {
            allow_precompiled: args.allow_precompiled,
            call_timeout: args.call_timeout,
            stack_switching: args.stack_switching,
            memory_limit: args.memory_limit,
        },
        gl_lib: args.gl_lib.as_ref().map(std::path::PathBuf::from),
        gl_software: args.gl_software,
        gl_stats: args.gl_stats,
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
            let copy_key = match args.copy_key.as_str() {
                "none" => None,
                k => match gasm_host::keymap::key_from_code(gasm_host::keymap::normalize(k)) {
                    Some(c) => Some(c),
                    None => return Err(format!("--copy-key: unknown key code {k:?} (use a KeyboardEvent.code name like F2, or none)")),
                },
            };
            // --icon, else the manifest's "icon" (an asset)
            let icon_png = match (&args.icon, session.manifest.as_ref().and_then(|m| m.icon.clone())) {
                (Some(p), _) => Some(std::fs::read(p).map_err(|e| format!("--icon {p}: {e}"))?),
                (None, Some(name)) => match session.assets.get(&name) {
                    Some(a) => {
                        let mut b = vec![0; a.size() as usize];
                        a.read_at(0, &mut b);
                        Some(b)
                    }
                    None => {
                        eprintln!("[gasm] icon: the manifest's icon {name:?} is not an asset");
                        None
                    }
                },
                _ => None,
            };
            let icon = icon_png.and_then(|b| match gasm_host::headless::decode_png(&b) {
                Ok(i) => Some(i),
                Err(e) => {
                    eprintln!("[gasm] icon: {e}");
                    None
                }
            });
            gasm_host::window::run(session, gasm_host::window::Options { size: args.window, keymap, mute: args.mute, present: args.present, screenshot: args.window_screenshot.clone(), splash: !args.no_splash && args.window_screenshot.is_none(), copy_key, icon, app_class: args.app_class.clone() })
        }
        #[cfg(not(feature = "window"))]
        None => Err("this gasm-run was built without the window feature: use --headless".into()),
    }
}
