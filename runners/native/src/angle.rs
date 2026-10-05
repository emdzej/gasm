//! ANGLE (OpenGL ES 3.0 on Metal, Direct3D 11 or Vulkan): the GL backend of
//! gasm:gl natively. libEGL and libGLESv2 are loaded at run time (scripts/fetch-angle.sh
//! puts them in tools/angle/<platform>; release bundles ship them next to gasm-run),
//! so gasm-run runs everything else without them.
//!
//! The context is the one Chrome gives WebGL 2: GLES 3.0 in ANGLE's WebGL
//! compatibility mode (WebGL's validation rules, extensions only on request),
//! robust buffer access and zero-initialised resources. The default framebuffer
//! has no alpha, depth 24 + stencil 8 and no multisampling, as in the browser runner.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use crate::gles::Gles;

type EglDisplay = *mut c_void;
type EglConfig = *mut c_void;
type EglContext = *mut c_void;
type EglSurface = *mut c_void;
type Attrib = isize;

const EGL_NONE: i32 = 0x3038;
const EGL_TRUE: u32 = 1;
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_DEPTH_SIZE: i32 = 0x3025;
const EGL_STENCIL_SIZE: i32 = 0x3026;
const EGL_SAMPLES: i32 = 0x3031;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_WINDOW_BIT: i32 = 0x0004;
const EGL_OPENGL_ES3_BIT: i32 = 0x0040;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_WIDTH: i32 = 0x3057;
const EGL_EXTENSIONS: i32 = 0x3055;
const EGL_VERSION: i32 = 0x3054;
const EGL_CONTEXT_MAJOR_VERSION: i32 = 0x3098;
const EGL_CONTEXT_MINOR_VERSION: i32 = 0x30FB;
const EGL_CONTEXT_OPENGL_ROBUST_ACCESS_EXT: i32 = 0x30BF;
const EGL_CONTEXT_WEBGL_COMPATIBILITY_ANGLE: i32 = 0x33AC;
const EGL_ROBUST_RESOURCE_INITIALIZATION_ANGLE: i32 = 0x3453;
const EGL_PLATFORM_ANGLE_ANGLE: u32 = 0x3202;
const EGL_PLATFORM_ANGLE_TYPE_ANGLE: Attrib = 0x3203;
const EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE: Attrib = 0x3208;
const EGL_PLATFORM_ANGLE_TYPE_VULKAN_ANGLE: Attrib = 0x3450;
const EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE: Attrib = 0x3489;
const EGL_PLATFORM_ANGLE_DEVICE_TYPE_ANGLE: Attrib = 0x3209;
const EGL_PLATFORM_ANGLE_DEVICE_TYPE_SWIFTSHADER_ANGLE: Attrib = 0x3487;
const EGL_PLATFORM_ANGLE_NATIVE_PLATFORM_TYPE_ANGLE: Attrib = 0x348F;
const EGL_PLATFORM_X11_EXT: Attrib = 0x31D5;
const EGL_PLATFORM_WAYLAND_EXT: Attrib = 0x31D8;
const EGL_PLATFORM_SURFACELESS_MESA: Attrib = 0x31DD;

/// The EGL entry points gasm uses.
#[allow(non_snake_case)]
struct Egl {
    GetProcAddress: unsafe extern "system" fn(*const c_char) -> *const c_void,
    GetPlatformDisplay: unsafe extern "system" fn(u32, *mut c_void, *const Attrib) -> EglDisplay,
    Initialize: unsafe extern "system" fn(EglDisplay, *mut i32, *mut i32) -> u32,
    Terminate: unsafe extern "system" fn(EglDisplay) -> u32,
    QueryString: unsafe extern "system" fn(EglDisplay, i32) -> *const c_char,
    ChooseConfig: unsafe extern "system" fn(EglDisplay, *const i32, *mut EglConfig, i32, *mut i32) -> u32,
    CreateContext: unsafe extern "system" fn(EglDisplay, EglConfig, EglContext, *const i32) -> EglContext,
    DestroyContext: unsafe extern "system" fn(EglDisplay, EglContext) -> u32,
    CreatePbufferSurface: unsafe extern "system" fn(EglDisplay, EglConfig, *const i32) -> EglSurface,
    CreateWindowSurface: unsafe extern "system" fn(EglDisplay, EglConfig, *mut c_void, *const i32) -> EglSurface,
    DestroySurface: unsafe extern "system" fn(EglDisplay, EglSurface) -> u32,
    QuerySurface: unsafe extern "system" fn(EglDisplay, EglSurface, i32, *mut i32) -> u32,
    MakeCurrent: unsafe extern "system" fn(EglDisplay, EglSurface, EglSurface, EglContext) -> u32,
    SwapBuffers: unsafe extern "system" fn(EglDisplay, EglSurface) -> u32,
    SwapInterval: unsafe extern "system" fn(EglDisplay, i32) -> u32,
    GetError: unsafe extern "system" fn() -> i32,
}

/// Where a window surface goes: the platform's native window.
#[derive(Clone, Copy)]
pub enum NativeWindow {
    /// macOS: an NSView (its layer gets the surface)
    AppKit(*mut c_void),
    /// Windows: an HWND
    Win32(*mut c_void),
    /// X11: the display and the window id
    Xlib(*mut c_void, std::ffi::c_ulong),
    /// Wayland: the display and a wl_egl_window is not available here: unsupported
    Wayland(*mut c_void),
}

/// One ANGLE display, context and surface, current on the thread that made it.
pub struct Angle {
    _lib: libloading::Library,
    egl: Egl,
    display: EglDisplay,
    context: EglContext,
    surface: EglSurface,
    pub gl: Gles,
    pub renderer: String,
}

// The context is used from one thread at a time (the runner's), as a GL context must be.
unsafe impl Send for Angle {}

const LIB_EGL: &str = if cfg!(target_os = "macos") {
    "libEGL.dylib"
} else if cfg!(windows) {
    "libEGL.dll"
} else {
    "libEGL.so"
};

const PLATFORM: &str = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "macos-arm64"
} else if cfg!(target_os = "macos") {
    "macos-x86_64"
} else if cfg!(windows) {
    "windows-x86_64"
} else if cfg!(target_arch = "aarch64") {
    "linux-arm64"
} else {
    "linux-x86_64"
};

/// The directory with ANGLE's libEGL: `dir` (--gl-lib), $GASM_ANGLE_DIR, next to the
/// executable (also ../Frameworks in a macOS .app, ./angle), or the repository's tools/angle.
pub fn find(dir: Option<&Path>) -> Result<PathBuf, String> {
    let mut tried = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = dir {
        candidates.push(d.to_path_buf());
    }
    if let Some(d) = std::env::var_os("GASM_ANGLE_DIR") {
        candidates.push(PathBuf::from(d));
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(exe.join("../Frameworks"));
        candidates.push(exe.clone());
        candidates.push(exe.join("angle"));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/angle").join(PLATFORM));
    for c in candidates {
        if c.join(LIB_EGL).is_file() {
            return Ok(c);
        }
        tried.push(c.display().to_string());
    }
    Err(format!(
        "ANGLE ({LIB_EGL}) not found; looked in {}. Run scripts/fetch-angle.sh, or pass --gl-lib DIR",
        tried.join(", ")
    ))
}

impl Angle {
    /// An offscreen context of `w`×`h` (headless screenshots).
    pub fn offscreen(dir: &Path, w: u32, h: u32, software: bool) -> Result<Angle, String> {
        choose(software, |backend| {
            Self::open(dir, None, software, backend, |egl, display, config| unsafe {
                let attrs = [EGL_WIDTH, w as i32, EGL_HEIGHT, h as i32, EGL_NONE];
                (egl.CreatePbufferSurface)(display, config, attrs.as_ptr())
            })
        })
    }

    /// A context drawing into `window`.
    pub fn window(dir: &Path, window: NativeWindow, software: bool) -> Result<Angle, String> {
        let native = match window {
            NativeWindow::AppKit(view) => layer_of(view),
            NativeWindow::Win32(hwnd) => hwnd,
            NativeWindow::Xlib(_, id) => id as usize as *mut c_void,
            NativeWindow::Wayland(_) => return Err("gasm:gl: Wayland windows aren't supported yet; run with WAYLAND_DISPLAY= (X11)".into()),
        };
        choose(software, |backend| {
            Self::open(dir, Some(window), software, backend, |egl, display, config| unsafe {
                (egl.CreateWindowSurface)(display, config, native, [EGL_NONE].as_ptr())
            })
        })
    }

    fn open(
        dir: &Path,
        window: Option<NativeWindow>,
        software: bool,
        backend: Attrib,
        surface: impl FnOnce(&Egl, EglDisplay, EglConfig) -> EglSurface,
    ) -> Result<Angle, String> {
        if software {
            // SwiftShader is a Vulkan driver next to ANGLE: point the Vulkan loader at it
            // (Chrome does the same); only this process, before ANGLE loads Vulkan
            let icd = dir.join("vk_swiftshader_icd.json");
            unsafe {
                std::env::set_var("VK_ICD_FILENAMES", &icd);
                std::env::set_var("VK_DRIVER_FILES", &icd);
            }
        }
        let path = dir.join(LIB_EGL);
        let lib = unsafe { libloading::Library::new(&path) }.map_err(|e| format!("{}: {e}", path.display()))?;
        let egl = unsafe { load_egl(&lib) }?;
        unsafe {
            let mut attrs: Vec<Attrib> = Vec::new();
            attrs.extend([EGL_PLATFORM_ANGLE_TYPE_ANGLE, backend]);
            if software {
                attrs.extend([EGL_PLATFORM_ANGLE_DEVICE_TYPE_ANGLE, EGL_PLATFORM_ANGLE_DEVICE_TYPE_SWIFTSHADER_ANGLE]);
            }
            let mut native_display = std::ptr::null_mut();
            if cfg!(all(unix, not(target_os = "macos"))) {
                match window {
                    Some(NativeWindow::Xlib(d, _)) => {
                        attrs.extend([EGL_PLATFORM_ANGLE_NATIVE_PLATFORM_TYPE_ANGLE, EGL_PLATFORM_X11_EXT]);
                        native_display = d;
                    }
                    Some(NativeWindow::Wayland(d)) => {
                        attrs.extend([EGL_PLATFORM_ANGLE_NATIVE_PLATFORM_TYPE_ANGLE, EGL_PLATFORM_WAYLAND_EXT]);
                        native_display = d;
                    }
                    _ => attrs.extend([EGL_PLATFORM_ANGLE_NATIVE_PLATFORM_TYPE_ANGLE, EGL_PLATFORM_SURFACELESS_MESA]),
                }
            }
            attrs.push(EGL_NONE as Attrib);
            let display = (egl.GetPlatformDisplay)(EGL_PLATFORM_ANGLE_ANGLE, native_display, attrs.as_ptr());
            if display.is_null() {
                return Err(format!("ANGLE: no display (EGL error {:#x})", (egl.GetError)()));
            }
            let (mut major, mut minor) = (0, 0);
            if (egl.Initialize)(display, &mut major, &mut minor) != EGL_TRUE {
                return Err(format!("ANGLE: eglInitialize failed (EGL error {:#x})", (egl.GetError)()));
            }
            let exts = cstr((egl.QueryString)(display, EGL_EXTENSIONS));
            if !exts.contains("EGL_ANGLE_create_context_webgl_compatibility") {
                (egl.Terminate)(display);
                return Err("ANGLE: no WebGL compatibility contexts (EGL_ANGLE_create_context_webgl_compatibility)".into());
            }
            let config = choose_config(&egl, display, if window.is_some() { EGL_WINDOW_BIT } else { EGL_PBUFFER_BIT })?;
            let mut ctx_attrs = vec![
                EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 0,
                EGL_CONTEXT_WEBGL_COMPATIBILITY_ANGLE, 1,
            ];
            if exts.contains("EGL_ANGLE_robust_resource_initialization") {
                ctx_attrs.extend([EGL_ROBUST_RESOURCE_INITIALIZATION_ANGLE, 1]);
            }
            if exts.contains("EGL_EXT_create_context_robustness") {
                ctx_attrs.extend([EGL_CONTEXT_OPENGL_ROBUST_ACCESS_EXT, 1]);
            }
            ctx_attrs.push(EGL_NONE);
            let context = (egl.CreateContext)(display, config, std::ptr::null_mut(), ctx_attrs.as_ptr());
            if context.is_null() {
                let e = (egl.GetError)();
                (egl.Terminate)(display);
                return Err(format!("ANGLE: cannot create a GLES 3.0 context (EGL error {e:#x})"));
            }
            let surf = surface(&egl, display, config);
            if surf.is_null() {
                let e = (egl.GetError)();
                (egl.DestroyContext)(display, context);
                (egl.Terminate)(display);
                return Err(format!("ANGLE: cannot create the surface (EGL error {e:#x})"));
            }
            if (egl.MakeCurrent)(display, surf, surf, context) != EGL_TRUE {
                return Err(format!("ANGLE: eglMakeCurrent failed (EGL error {:#x})", (egl.GetError)()));
            }
            if window.is_some() {
                (egl.SwapInterval)(display, 0); // the runner paces frames
            }
            let get = egl.GetProcAddress;
            let gl = Gles::load(|name: &CStr| get(name.as_ptr()))?;
            let renderer = cstr((gl.glGetString)(0x1F01) as *const c_char);
            let version = cstr((egl.QueryString)(display, EGL_VERSION));
            let renderer = format!("{renderer} (EGL {version})");
            Ok(Angle { _lib: lib, egl, display, context, surface: surf, gl, renderer })
        }
    }

    /// The surface size in pixels (a window surface follows the window).
    pub fn size(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        unsafe {
            (self.egl.QuerySurface)(self.display, self.surface, EGL_WIDTH, &mut w);
            (self.egl.QuerySurface)(self.display, self.surface, EGL_HEIGHT, &mut h);
        }
        (w.max(0) as u32, h.max(0) as u32)
    }

    /// Show an RGBA8 image (rows top to bottom) letterboxed on black and swap: the
    /// runner's own frames (the splash) before the game draws. Leaves no GL state behind
    /// that the game could see (its objects are deleted, bindings reset to 0).
    pub fn show_image(&self, rgba: &[u8], w: u32, h: u32) {
        const TEXTURE_2D: u32 = 0x0DE1;
        const READ_FRAMEBUFFER: u32 = 0x8CA8;
        const DRAW_FRAMEBUFFER: u32 = 0x8CA9;
        const COLOR_ATTACHMENT0: u32 = 0x8CE0;
        const SCISSOR_TEST: u32 = 0x0C11;
        let g = &self.gl;
        let (dw, dh) = self.size();
        let (ox, oy, sx, sy) = crate::present::letterbox((dw as f64, dh as f64), (w as f64, h as f64), true, None);
        let (x0, x1) = (ox.round() as i32, (ox + sx * w as f64).round() as i32);
        // GL counts rows from the bottom: the image's first row goes to the top
        let (y0, y1) = ((dh as f64 - oy).round() as i32, (dh as f64 - oy - sy * h as f64).round() as i32);
        unsafe {
            let (mut tex, mut fbo) = (0u32, 0u32);
            (g.glGenTextures)(1, &mut tex as *mut u32 as *mut c_void);
            (g.glBindTexture)(TEXTURE_2D, tex);
            (g.glPixelStorei)(0x0CF5, 1); // UNPACK_ALIGNMENT
            (g.glTexImage2D)(TEXTURE_2D, 0, 0x8058 /* RGBA8 */ as i32, w as i32, h as i32, 0, 0x1908 /* RGBA */, 0x1401 /* UNSIGNED_BYTE */, rgba.as_ptr() as *const c_void);
            (g.glPixelStorei)(0x0CF5, 4);
            (g.glGenFramebuffers)(1, &mut fbo as *mut u32 as *mut c_void);
            (g.glBindFramebuffer)(READ_FRAMEBUFFER, fbo);
            (g.glFramebufferTexture2D)(READ_FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, tex, 0);
            (g.glBindFramebuffer)(DRAW_FRAMEBUFFER, 0);
            (g.glDisable)(SCISSOR_TEST);
            (g.glClearColor)(0.0, 0.0, 0.0, 1.0);
            (g.glClear)(0x4000); // COLOR_BUFFER_BIT
            (g.glBlitFramebuffer)(0, 0, w as i32, h as i32, x0, y0, x1, y1, 0x4000, 0x2600 /* NEAREST */);
            (g.glBindFramebuffer)(READ_FRAMEBUFFER, 0);
            (g.glBindTexture)(TEXTURE_2D, 0);
            (g.glDeleteFramebuffers)(1, &fbo as *const u32 as *const c_void);
            (g.glDeleteTextures)(1, &tex as *const u32 as *const c_void);
        }
        self.swap();
    }

    pub fn swap(&self) {
        unsafe {
            (self.egl.SwapBuffers)(self.display, self.surface);
        }
    }

    pub fn make_current(&self) {
        unsafe {
            (self.egl.MakeCurrent)(self.display, self.surface, self.surface, self.context);
        }
    }

    /// The default framebuffer as RGBA8 rows, top to bottom.
    pub fn read_frame(&self) -> (u32, u32, Vec<u8>) {
        let (w, h) = self.size();
        let mut px = vec![0u8; w as usize * h as usize * 4];
        unsafe {
            let g = &self.gl;
            let (mut fb, mut pack, mut align) = (0, 0, 0);
            (g.glGetIntegerv)(0x8CAA, &mut fb as *mut i32 as *mut c_void); // READ_FRAMEBUFFER_BINDING
            (g.glGetIntegerv)(0x88ED, &mut pack as *mut i32 as *mut c_void); // PIXEL_PACK_BUFFER_BINDING
            (g.glGetIntegerv)(0x0D05, &mut align as *mut i32 as *mut c_void); // PACK_ALIGNMENT
            (g.glBindFramebuffer)(0x8CA8, 0);
            (g.glBindBuffer)(0x88EB, 0);
            (g.glPixelStorei)(0x0D05, 1);
            (g.glReadPixels)(0, 0, w as i32, h as i32, 0x1908, 0x1401, px.as_mut_ptr() as *mut c_void);
            (g.glPixelStorei)(0x0D05, align);
            (g.glBindBuffer)(0x88EB, pack as u32);
            (g.glBindFramebuffer)(0x8CA8, fb as u32);
        }
        let row = w as usize * 4;
        let mut flipped = Vec::with_capacity(px.len());
        for y in (0..h as usize).rev() {
            flipped.extend_from_slice(&px[y * row..(y + 1) * row]);
        }
        (w, h, flipped)
    }
}

impl Drop for Angle {
    fn drop(&mut self) {
        unsafe {
            (self.egl.MakeCurrent)(self.display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (self.egl.DestroySurface)(self.display, self.surface);
            (self.egl.DestroyContext)(self.display, self.context);
            (self.egl.Terminate)(self.display);
        }
    }
}

unsafe fn choose_config(egl: &Egl, display: EglDisplay, surface_bit: i32) -> Result<EglConfig, String> {
    for alpha in [0, 8] {
        let attrs = [
            EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, alpha,
            EGL_DEPTH_SIZE, 24, EGL_STENCIL_SIZE, 8, EGL_SAMPLES, 0,
            EGL_SURFACE_TYPE, surface_bit, EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT, EGL_NONE,
        ];
        let mut config = std::ptr::null_mut();
        let mut n = 0;
        if unsafe { (egl.ChooseConfig)(display, attrs.as_ptr(), &mut config, 1, &mut n) } == EGL_TRUE && n > 0 {
            return Ok(config);
        }
    }
    Err("ANGLE: no RGB8 + depth 24 + stencil 8 config".into())
}

unsafe fn load_egl(lib: &libloading::Library) -> Result<Egl, String> {
    unsafe fn sym<T: Copy>(lib: &libloading::Library, name: &str) -> Result<T, String> {
        let s: libloading::Symbol<T> = unsafe { lib.get(name.as_bytes()) }.map_err(|e| format!("ANGLE: {name}: {e}"))?;
        Ok(*s)
    }
    unsafe {
        Ok(Egl {
            GetProcAddress: sym(lib, "eglGetProcAddress")?,
            GetPlatformDisplay: sym(lib, "eglGetPlatformDisplay")?,
            Initialize: sym(lib, "eglInitialize")?,
            Terminate: sym(lib, "eglTerminate")?,
            QueryString: sym(lib, "eglQueryString")?,
            ChooseConfig: sym(lib, "eglChooseConfig")?,
            CreateContext: sym(lib, "eglCreateContext")?,
            DestroyContext: sym(lib, "eglDestroyContext")?,
            CreatePbufferSurface: sym(lib, "eglCreatePbufferSurface")?,
            CreateWindowSurface: sym(lib, "eglCreateWindowSurface")?,
            DestroySurface: sym(lib, "eglDestroySurface")?,
            QuerySurface: sym(lib, "eglQuerySurface")?,
            MakeCurrent: sym(lib, "eglMakeCurrent")?,
            SwapBuffers: sym(lib, "eglSwapBuffers")?,
            SwapInterval: sym(lib, "eglSwapInterval")?,
            GetError: sym(lib, "eglGetError")?,
        })
    }
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// A NUL-terminated copy for GL (names never contain NUL: they come from UTF-8 checked strings).
pub fn c_string(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("no NUL")
}

/// The ANGLE backend: $GASM_ANGLE_BACKEND (metal, opengl, vulkan, d3d11), else the
/// platform's (Vulkan for SwiftShader). Metal on a virtual machine's paravirtual GPU
/// lacks what ANGLE needs (argument encoders; it crashes on the first draw), so
/// there ANGLE's OpenGL backend is used instead.
fn choose(software: bool, open: impl Fn(Attrib) -> Result<Angle, String>) -> Result<Angle, String> {
    const OPENGL: Attrib = 0x320D; // EGL_PLATFORM_ANGLE_TYPE_OPENGL_ANGLE
    let chosen = std::env::var("GASM_ANGLE_BACKEND").ok();
    let backend = match chosen.as_deref() {
        Some("metal") => EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE,
        Some("opengl") => OPENGL,
        Some("vulkan") => EGL_PLATFORM_ANGLE_TYPE_VULKAN_ANGLE,
        Some("d3d11") => EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE,
        Some(other) => return Err(format!("GASM_ANGLE_BACKEND={other}: use metal, opengl, vulkan or d3d11")),
        None if software => EGL_PLATFORM_ANGLE_TYPE_VULKAN_ANGLE,
        None if cfg!(target_os = "macos") => EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE,
        None if cfg!(windows) => EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE,
        None => EGL_PLATFORM_ANGLE_TYPE_VULKAN_ANGLE,
    };
    let a = open(backend)?;
    if chosen.is_none() && backend == EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE && a.renderer.contains("Paravirtual") {
        drop(a);
        eprintln!("[gasm] gl: paravirtual GPU (a virtual machine): ANGLE on OpenGL instead of Metal");
        return open(OPENGL);
    }
    Ok(a)
}

/// macOS: the NSView's layer (made layer-backed first): ANGLE draws into a sublayer of it.
#[cfg(target_os = "macos")]
fn layer_of(view: *mut c_void) -> *mut c_void {
    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }
    unsafe {
        let send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, u8) = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let send_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        send_bool(view, sel_registerName(c"setWantsLayer:".as_ptr()), 1);
        send_ptr(view, sel_registerName(c"layer".as_ptr()))
    }
}

#[cfg(not(target_os = "macos"))]
fn layer_of(view: *mut c_void) -> *mut c_void {
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs where ANGLE is available (scripts/fetch-angle.sh); skipped otherwise.
    #[test]
    fn offscreen_context() {
        let Ok(dir) = find(None) else { return eprintln!("ANGLE not found: skipped") };
        let a = Angle::offscreen(&dir, 64, 32, false).or_else(|_| Angle::offscreen(&dir, 64, 32, true)).expect("context");
        assert_eq!(a.size(), (64, 32));
        eprintln!("{}", a.renderer);
        unsafe {
            (a.gl.glClearColor)(1.0, 0.5, 0.0, 1.0);
            (a.gl.glClear)(0x4000);
        }
        let (w, h, px) = a.read_frame();
        assert_eq!((w, h), (64, 32));
        assert_eq!(&px[..4], &[255, 128, 0, 255]);
    }
}
