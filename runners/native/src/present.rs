//! Showing `video_present` frames: the letterbox and the upscaling filters
//! (design/presentation.md). Display only: hashes are taken from the guest's
//! bytes before any of this runs.

use crate::gfx::Gpu;

/// How a 2D frame is scaled to the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    /// square pixels, nearest neighbour (linear when shrinking)
    Nearest,
    /// integer prescale + bilinear: even pixels at any factor, exactly nearest
    /// at whole factors (the default)
    #[default]
    Sharp,
    /// edge-directed (xBR level 2): rounds diagonals of pixel art
    Xbr,
    /// AMD FSR 1 (EASU upscale + RCAS sharpen): rendered and dithered content
    Fsr,
    /// a CRT look: scanlines (a beam that widens with brightness) and an aperture grille
    Crt,
}

impl Filter {
    pub const NAMES: &str = "nearest, sharp, xbr, fsr, crt";

    pub fn parse(s: &str) -> Result<Filter, String> {
        match s {
            "nearest" => Ok(Filter::Nearest),
            "sharp" => Ok(Filter::Sharp),
            "xbr" => Ok(Filter::Xbr),
            "fsr" => Ok(Filter::Fsr),
            "crt" => Ok(Filter::Crt),
            _ => Err(format!("unknown filter {s:?} (use {})", Filter::NAMES)),
        }
    }
}

/// The user's display settings for 2D frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Present {
    pub filter: Filter,
    /// scale by whole multiples only (black border around), when the frame fits
    pub integer_scale: bool,
}

/// Where a `fw×fh` frame lands in a `dw×dh` output: (left, top, scale x, scale y),
/// centred. `integer_scale`: whole multiples only (of the height), when the frame
/// fits; `aspect`: the frame's display aspect num:den (`video_set_aspect`), None
/// for square pixels. The pointer's frame position undoes exactly this
/// (`host::frame_position`); same arithmetic in runners/web/lib/input.js.
pub fn letterbox(drawable: (f64, f64), frame: (f64, f64), integer_scale: bool, aspect: Option<(u32, u32)>) -> (f64, f64, f64, f64) {
    let (dw, dh, fw, fh) = (drawable.0, drawable.1, frame.0, frame.1);
    let Some((n, d)) = aspect.map(|(n, d)| (n as f64, d as f64)) else {
        let mut scale = (dw / fw).min(dh / fh);
        if integer_scale && scale >= 1.0 {
            scale = scale.floor();
        }
        return ((dw - fw * scale) / 2.0, (dh - fh * scale) / 2.0, scale, scale);
    };
    // shown fh*n/d wide per fh high; whole numbers are multiplied before dividing,
    // so e.g. 320x200 at 4:3 gives exact scales
    let mut scale = (dw * d / (fh * n)).min(dh / fh);
    if integer_scale && scale >= 1.0 {
        scale = scale.floor();
    }
    let sx = scale * fh * n / (d * fw);
    ((dw - fw * sx) / 2.0, (dh - fh * scale) / 2.0, sx, scale)
}

/// The filter that actually runs at a scale factor: shrinking is always linear
/// (`sharp` degrades to bilinear), and xBR needs room to draw its edges. FSR
/// upscales at any factor above 1; scanlines need two output rows per line.
fn effective(filter: Filter, scale: f64) -> Filter {
    match filter {
        _ if scale < 1.0 => Filter::Sharp,
        Filter::Xbr if scale < 1.5 => Filter::Sharp,
        Filter::Fsr if scale <= 1.0 => Filter::Sharp,
        Filter::Crt if scale < 2.0 => Filter::Sharp,
        f => f,
    }
}

const SHADER: &str = r#"
// src: frame size; out, origin: the letterboxed viewport (whole pixels)
struct U { src: vec2<f32>, out: vec2<f32>, origin: vec2<f32> };
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var lin: sampler;
@group(0) @binding(2) var<uniform> u: U;

// one triangle covering the viewport
struct VO { @builtin(position) pos: vec4<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VO {
  let c = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  return VO(vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0));
}

// The fragment's position in frame pixels, from its exact framebuffer position
// (an interpolated uv rounds differently on different GPUs, and at some scales
// pixel centres fall exactly on texel edges). The bias sends such ties the same
// way everywhere; gasm-present.js does the same.
fn frame_pos(i: VO) -> vec2<f32> {
  return (i.pos.xy - u.origin) * u.src / u.out + 1.0 / 1024.0;
}

@fragment fn fs_nearest(i: VO) -> @location(0) vec4<f32> {
  let size = vec2<i32>(textureDimensions(t));
  return textureLoad(t, clamp(vec2<i32>(floor(frame_pos(i))), vec2<i32>(0), size - 1), 0);
}

// Sharp bilinear: as if the frame were first scaled by the largest whole factor
// with nearest neighbour, then bilinearly to the output. Each pixel stays flat
// and only its border, about one output pixel wide, blends with the next one.
@fragment fn fs_sharp(i: VO) -> @location(0) vec4<f32> {
  let texel = frame_pos(i);
  let scale = max(floor(u.out / u.src), vec2<f32>(1.0));
  let f = fract(texel) - 0.5;
  let flat_half = 0.5 - 0.5 / scale + 1.0 / 512.0; // + the bias: whole factors give exactly nearest
  let g = (f - clamp(f, -flat_half, flat_half)) * scale + 0.5;
  return textureSampleLevel(t, lin, (floor(texel) + g) / u.src, 0.0);
}

// xBR level 2 (the algorithm of Hyllian's xBR-lv2 shaders, MIT; this is an
// independent implementation). Per output pixel, look at the 21 source pixels
// around it, find edges through each of the four corners of the centre pixel
// E (at 30, 45 and 60 degrees) and blend E with the neighbour across the edge.
// Any scale in one pass; the edge is antialiased over one output pixel.
// Vectors hold the four corners (bottom-right, top-right, top-left,
// bottom-left), each the previous one rotated by 90 degrees.
const Y = vec3<f32>(14.352, 28.176, 5.472); // luma weights (x48)
const EQ_THRESHOLD = 15.0;
const LV2_COEFFICIENT = 2.0;

fn px(c: vec2<i32>, dx: i32, dy: i32) -> vec3<f32> {
  let size = vec2<i32>(textureDimensions(t));
  return textureLoad(t, clamp(c + vec2<i32>(dx, dy), vec2<i32>(0), size - 1), 0).rgb;
}
fn df(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> { return abs(a - b); }
fn eq(a: vec4<f32>, b: vec4<f32>) -> vec4<bool> { return df(a, b) < vec4<f32>(EQ_THRESHOLD); }
fn wd(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, e: vec4<f32>, f: vec4<f32>, g: vec4<f32>, h: vec4<f32>) -> vec4<f32> {
  return df(a, b) + df(a, c) + df(d, e) + df(d, f) + 4.0 * df(g, h);
}
fn on(b: vec4<bool>) -> vec4<f32> { return select(vec4<f32>(0.0), vec4<f32>(1.0), b); }

@fragment fn fs_xbr(i: VO) -> @location(0) vec4<f32> {
  let pos = frame_pos(i);
  let tc = floor(pos);
  let fp = pos - tc;
  let c0 = vec2<i32>(tc);
  //     A1 B1 C1
  //  A0 A  B  C  C4
  //  D0 D  E  F  F4
  //  G0 G  H  I  I4
  //     G5 H5 I5
  let A1 = px(c0, -1, -2); let B1 = px(c0, 0, -2); let C1 = px(c0, 1, -2);
  let A0 = px(c0, -2, -1); let A = px(c0, -1, -1); let B = px(c0, 0, -1); let C = px(c0, 1, -1); let C4 = px(c0, 2, -1);
  let D0 = px(c0, -2, 0); let D = px(c0, -1, 0); let E = px(c0, 0, 0); let F = px(c0, 1, 0); let F4 = px(c0, 2, 0);
  let G0 = px(c0, -2, 1); let G = px(c0, -1, 1); let H = px(c0, 0, 1); let I = px(c0, 1, 1); let I4 = px(c0, 2, 1);
  let G5 = px(c0, -1, 2); let H5 = px(c0, 0, 2); let I5 = px(c0, 1, 2);

  let b = vec4<f32>(dot(B, Y), dot(D, Y), dot(H, Y), dot(F, Y));
  let c = vec4<f32>(dot(C, Y), dot(A, Y), dot(G, Y), dot(I, Y));
  let e = vec4<f32>(dot(E, Y));
  let d = b.yzwx; let f = b.wxyz; let g = c.zwxy; let h = b.zwxy; let ii = c.wxyz;
  let i4 = vec4<f32>(dot(I4, Y), dot(C1, Y), dot(A0, Y), dot(G5, Y));
  let i5 = vec4<f32>(dot(I5, Y), dot(C4, Y), dot(A1, Y), dot(G0, Y));
  let h5 = vec4<f32>(dot(H5, Y), dot(F4, Y), dot(B1, Y), dot(D0, Y));
  let f4 = h5.yzwx;

  // position within E along each corner's 45 / 30 / 60 degree direction
  let fx = vec4<f32>(1.0, -1.0, -1.0, 1.0) * fp.y + vec4<f32>(1.0, 1.0, -1.0, -1.0) * fp.x;
  let fx_left = vec4<f32>(1.0, -1.0, -1.0, 1.0) * fp.y + vec4<f32>(0.5, 2.0, -0.5, -2.0) * fp.x;
  let fx_up = vec4<f32>(1.0, -1.0, -1.0, 1.0) * fp.y + vec4<f32>(2.0, 0.5, -2.0, -0.5) * fp.x;
  let co = vec4<f32>(1.5, 0.5, -0.5, 0.5);
  let cx = vec4<f32>(1.0, 1.0, -0.5, 0.0);
  let cy = vec4<f32>(2.0, 0.0, -1.0, 0.5);
  // half the edge's blend width: one output pixel, in source pixels
  let s = max(min(u.out.x / u.src.x, u.out.y / u.src.y), 1.0);
  let delta = vec4<f32>(1.0 / s);
  let delta_l = vec4<f32>(0.5, 1.0, 0.5, 1.0) / s;
  let delta_u = delta_l.yxwz;

  let lv0 = (e != f) & (e != h);
  let lv1 = lv0 & ((!eq(f, b) & !eq(h, d)) | (eq(e, ii) & !eq(f, i4) & !eq(h, i5)) | eq(e, g) | eq(e, c));
  let lv2_left = (e != g) & (d != g);
  let lv2_up = (e != c) & (b != c);
  let wd1 = wd(e, c, g, ii, h5, f4, h, f);
  let wd2 = wd(h, d, i5, f, i4, b, e, ii);
  let edri = (wd1 <= wd2) & lv0;
  let edr = (wd1 < wd2) & lv1;
  let edr_left = (LV2_COEFFICIENT * df(f, g) <= df(h, c)) & lv2_left & edr;
  let edr_up = (df(f, g) >= LV2_COEFFICIENT * df(h, c)) & lv2_up & edr;

  let fx45 = on(edr) * saturate((fx + delta - co) / (2.0 * delta));
  let fx45i = on(edri) * saturate((fx + delta - co - 0.25) / (2.0 * delta));
  let fx30 = on(edr_left) * saturate((fx_left + delta_l - cx) / (2.0 * delta_l));
  let fx60 = on(edr_up) * saturate((fx_up + delta_u - cy) / (2.0 * delta_u));
  let m = max(max(fx30, fx60), max(fx45, fx45i));
  // blend towards whichever of the two edge neighbours is closer to E
  let p = df(e, f) <= df(e, h);
  var r1 = mix(E, select(H, F, p.x), m.x);
  r1 = mix(r1, select(B, D, p.z), m.z);
  var r2 = mix(E, select(F, B, p.y), m.y);
  r2 = mix(r2, select(D, H, p.w), m.w);
  let d1 = dot(abs(E - r1), vec3<f32>(1.0));
  let d2 = dot(abs(E - r2), vec3<f32>(1.0));
  return vec4<f32>(select(r1, r2, d2 >= d1), 1.0);
}

// AMD FidelityFX Super Resolution 1 (MIT; an independent implementation of its
// two passes). EASU: 12 source pixels around the output pixel, the local edge
// direction and length from their luma, then a Lanczos-2-like kernel stretched
// along the edge, clamped to the 4 nearest pixels (no ringing). Renders into an
// intermediate texture the size of the viewport; RCAS then sharpens that.
fn luma(c: vec3<f32>) -> f32 { return c.b * 0.5 + (c.r * 0.5 + c.g); }

// one bilinear quadrant's contribution to direction and length; w: its weight,
// a/b/c/d/e: luma above, left of, at, right of and below the quadrant's pixel
fn easu_set(w: f32, a: f32, b: f32, c: f32, d: f32, e: f32) -> vec3<f32> {
  let dc = d - c; let cb = c - b;
  var lx = max(abs(dc), abs(cb));
  lx = select(1.0 / lx, 0.0, lx == 0.0);
  let dir_x = d - b;
  lx = saturate(abs(dir_x) * lx); lx = lx * lx;
  let ec = e - c; let ca = c - a;
  var ly = max(abs(ec), abs(ca));
  ly = select(1.0 / ly, 0.0, ly == 0.0);
  let dir_y = e - a;
  ly = saturate(abs(dir_y) * ly); ly = ly * ly;
  return vec3<f32>(dir_x * w, dir_y * w, (lx + ly) * w);
}

fn easu_tap(off: vec2<f32>, dir: vec2<f32>, len2: vec2<f32>, lob: f32, clp: f32, c: vec3<f32>) -> vec4<f32> {
  var v = vec2<f32>(off.x * dir.x + off.y * dir.y, off.x * -dir.y + off.y * dir.x);
  v = v * len2;
  let d2 = min(v.x * v.x + v.y * v.y, clp);
  var wb = 0.4 * d2 - 1.0;
  var wa = lob * d2 - 1.0;
  wb = wb * wb; wa = wa * wa;
  wb = 1.5625 * wb - 0.5625;
  let w = wb * wa;
  return vec4<f32>(c * w, w);
}

@fragment fn fs_easu(i: VO) -> @location(0) vec4<f32> {
  // source position with pixel centres at whole numbers
  let pp0 = frame_pos(i) - 0.5;
  let fp = floor(pp0);
  let pp = pp0 - fp;
  let c0 = vec2<i32>(fp);
  //    b c
  //  e f g h
  //  i j k l
  //    n o
  let b = px(c0, 0, -1); let c = px(c0, 1, -1);
  let e = px(c0, -1, 0); let f = px(c0, 0, 0); let g = px(c0, 1, 0); let h = px(c0, 2, 0);
  let ii = px(c0, -1, 1); let j = px(c0, 0, 1); let k = px(c0, 1, 1); let l = px(c0, 2, 1);
  let n = px(c0, 0, 2); let o = px(c0, 1, 2);
  let bl = luma(b); let cl = luma(c); let el = luma(e); let fl = luma(f); let gl = luma(g); let hl = luma(h);
  let il = luma(ii); let jl = luma(j); let kl = luma(k); let ll = luma(l); let nl = luma(n); let ol = luma(o);
  var acc = easu_set((1.0 - pp.x) * (1.0 - pp.y), bl, el, fl, gl, jl);
  acc = acc + easu_set(pp.x * (1.0 - pp.y), cl, fl, gl, hl, kl);
  acc = acc + easu_set((1.0 - pp.x) * pp.y, fl, il, jl, kl, nl);
  acc = acc + easu_set(pp.x * pp.y, gl, jl, kl, ll, ol);
  var dir = acc.xy;
  var len = acc.z;
  let dr = dir.x * dir.x + dir.y * dir.y;
  let zero = dr < 1.0 / 32768.0;
  dir = select(dir * inverseSqrt(dr), vec2<f32>(1.0, 0.0), zero);
  len = len * 0.5; len = len * len;
  let stretch = (dir.x * dir.x + dir.y * dir.y) / max(abs(dir.x), abs(dir.y));
  let len2 = vec2<f32>(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
  let lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
  let clp = 1.0 / lob;
  var a = vec4<f32>(0.0);
  a = a + easu_tap(vec2<f32>(0.0, -1.0) - pp, dir, len2, lob, clp, b);
  a = a + easu_tap(vec2<f32>(1.0, -1.0) - pp, dir, len2, lob, clp, c);
  a = a + easu_tap(vec2<f32>(-1.0, 1.0) - pp, dir, len2, lob, clp, ii);
  a = a + easu_tap(vec2<f32>(0.0, 1.0) - pp, dir, len2, lob, clp, j);
  a = a + easu_tap(vec2<f32>(0.0, 0.0) - pp, dir, len2, lob, clp, f);
  a = a + easu_tap(vec2<f32>(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
  a = a + easu_tap(vec2<f32>(1.0, 1.0) - pp, dir, len2, lob, clp, k);
  a = a + easu_tap(vec2<f32>(2.0, 1.0) - pp, dir, len2, lob, clp, l);
  a = a + easu_tap(vec2<f32>(2.0, 0.0) - pp, dir, len2, lob, clp, h);
  a = a + easu_tap(vec2<f32>(1.0, 0.0) - pp, dir, len2, lob, clp, g);
  a = a + easu_tap(vec2<f32>(1.0, 2.0) - pp, dir, len2, lob, clp, o);
  a = a + easu_tap(vec2<f32>(0.0, 2.0) - pp, dir, len2, lob, clp, n);
  let mn = min(min(f, g), min(j, k));
  let mx = max(max(f, g), max(j, k));
  return vec4<f32>(min(mx, max(mn, a.rgb / a.w)), 1.0);
}

// CRT (own design): each output pixel sees the two nearest source lines as
// beams of light whose height grows with their brightness, horizontally as
// sharp bilinear; lit in linear light, then an aperture grille (RGB stripes,
// one output pixel each) and a boost for the light the dark gaps take away.
const CRT_MASK = 0.3;   // how much a stripe dims the other two colours
const CRT_BOOST = 1.45;
fn crt_line(x: f32, line: f32) -> vec3<f32> {
  let c = textureSampleLevel(t, lin, vec2<f32>(x, line + 0.5) / u.src, 0.0).rgb;
  return pow(c, vec3<f32>(2.4));
}
fn crt_beam(d: f32, c: vec3<f32>) -> vec3<f32> {
  let width = mix(vec3<f32>(0.3), vec3<f32>(0.55), sqrt(c));
  let e = d / width;
  return exp(-e * e);
}
@fragment fn fs_crt(i: VO) -> @location(0) vec4<f32> {
  let pos = frame_pos(i);
  let sx = max(floor(u.out.x / u.src.x), 1.0);
  let f = fract(pos.x) - 0.5;
  let flat_half = 0.5 - 0.5 / sx + 1.0 / 512.0;
  let x = floor(pos.x) + (f - clamp(f, -flat_half, flat_half)) * sx + 0.5;
  let y = pos.y - 0.5;
  let line = floor(y);
  let d = y - line;
  let c0 = crt_line(x, line);
  let c1 = crt_line(x, min(line + 1.0, u.src.y - 1.0));
  var col = c0 * crt_beam(d, c0) + c1 * crt_beam(1.0 - d, c1);
  let stripe = u32(floor(i.pos.x)) % 3u;
  let mask = select(vec3<f32>(1.0 - CRT_MASK), vec3<f32>(1.0), vec3<u32>(0u, 1u, 2u) == vec3<u32>(stripe));
  col = col * mask * CRT_BOOST;
  return vec4<f32>(pow(min(col, vec3<f32>(1.0)), vec3<f32>(1.0 / 2.4)), 1.0);
}

// RCAS: sharpen the EASU output (t is now that intermediate texture, one texel
// per viewport pixel) with a 5-tap cross, as strong as the neighbourhood allows
// without clipping. 0.2 stops below maximum sharpness, FSR's default.
const RCAS_SHARPNESS = 0.87055056; // exp2(-0.2)
const RCAS_LIMIT = 0.1875;          // 0.25 - 1/16
@fragment fn fs_rcas(i: VO) -> @location(0) vec4<f32> {
  let c0 = vec2<i32>(floor(i.pos.xy - u.origin));
  let b = px(c0, 0, -1); let d = px(c0, -1, 0); let e = px(c0, 0, 0); let f = px(c0, 1, 0); let h = px(c0, 0, 1);
  let mn = min(min(b, d), min(f, h));
  let mx = max(max(b, d), max(f, h));
  let hit_min = mn / max(4.0 * mx, vec3<f32>(1.0 / 1024.0));
  let hit_max = (1.0 - mx) / min(4.0 * mn - 4.0, vec3<f32>(-1.0 / 1024.0));
  let lobe_rgb = max(-hit_min, hit_max);
  let lobe = max(-RCAS_LIMIT, min(max(lobe_rgb.r, max(lobe_rgb.g, lobe_rgb.b)), 0.0)) * RCAS_SHARPNESS;
  return vec4<f32>((lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0), 1.0);
}
"#;

/// A `video_present` frame to show: tightly packed RGBA rows and its display aspect.
pub struct VideoFrame<'a> {
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// `video_set_aspect` (num, den); None: square pixels
    pub aspect: Option<(u32, u32)>,
}

struct Source {
    texture: wgpu::Texture,
    /// with `uniform` (drawn into the output) and with `first` (drawn into `mid`)
    bind_group: wgpu::BindGroup,
    bind_group_first: wgpu::BindGroup,
    size: (u32, u32),
}

/// The intermediate image of two-pass filters (FSR), one texel per viewport pixel.
struct Mid {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// GPU side of showing 2D frames: one pipeline per filter pass, the frame
/// texture, the intermediate image of two-pass filters and two uniforms with the
/// sizes (for the pass into the output, and for a first pass into `mid`).
pub struct Presenter {
    nearest: wgpu::RenderPipeline,
    sharp: wgpu::RenderPipeline,
    xbr: wgpu::RenderPipeline,
    easu: wgpu::RenderPipeline,
    rcas: wgpu::RenderPipeline,
    crt: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    uniform: wgpu::Buffer,
    first: wgpu::Buffer,
    source: Option<Source>,
    mid: Option<Mid>,
}

const MID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Presenter {
    pub fn new(gpu: &Gpu) -> Presenter {
        let dev = &gpu.device;
        let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("present"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let frag = wgpu::ShaderStages::FRAGMENT;
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: frag, ty, count: None };
        let layout = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present"),
            entries: &[
                entry(0, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }),
                entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(2, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(24) }),
            ],
        });
        let pl = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("present"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |fs: &str, format: wgpu::TextureFormat| {
            dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(format.into())] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let uniform = || {
            dev.create_buffer(&wgpu::BufferDescriptor {
                label: Some("present"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Presenter {
            nearest: pipeline("fs_nearest", gpu.format),
            sharp: pipeline("fs_sharp", gpu.format),
            xbr: pipeline("fs_xbr", gpu.format),
            easu: pipeline("fs_easu", MID_FORMAT),
            rcas: pipeline("fs_rcas", gpu.format),
            crt: pipeline("fs_crt", gpu.format),
            linear: dev.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() }),
            uniform: uniform(),
            first: uniform(),
            layout,
            source: None,
            mid: None,
        }
    }

    fn bind_group(&self, gpu: &Gpu, view: &wgpu::TextureView, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.linear) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        })
    }

    /// Upload the frame (a new texture only when its size changes).
    fn upload(&mut self, gpu: &Gpu, rgba: &[u8], w: u32, h: u32) {
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        if !matches!(&self.source, Some(s) if s.size == (w, h)) {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind_group = self.bind_group(gpu, &view, &self.uniform);
            let bind_group_first = self.bind_group(gpu, &view, &self.first);
            self.source = Some(Source { texture, bind_group, bind_group_first, size: (w, h) });
        }
        let tex = &self.source.as_ref().unwrap().texture;
        gpu.queue.write_texture(
            tex.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            extent,
        );
    }

    /// Draw a `w×h` RGBA frame at display aspect `aspect` (None: square pixels)
    /// letterboxed into `view`, an output of `size` pixels.
    pub fn draw(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, size: (u32, u32), frame: VideoFrame, opts: Present) {
        let VideoFrame { rgba, width: w, height: h, aspect } = frame;
        self.upload(gpu, rgba, w, h);
        let (ox, oy, sx, sy) = letterbox((size.0 as f64, size.1 as f64), (w as f64, h as f64), opts.integer_scale, aspect);
        // whole pixels, as WebGL viewports are (gasm-present.js), so both runners show the same image
        let (vx, vy, vw, vh) = (ox.round() as f32, oy.round() as f32, (w as f64 * sx).round() as f32, (h as f64 * sy).round() as f32);
        let write = |buf: &wgpu::Buffer, origin: (f32, f32)| {
            let u: [f32; 8] = [w as f32, h as f32, vw, vh, origin.0, origin.1, 0.0, 0.0];
            gpu.queue.write_buffer(buf, 0, &u.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>());
        };
        write(&self.uniform, (vx, vy));
        let filter = effective(opts.filter, sx.min(sy));
        if filter == Filter::Fsr {
            // EASU into `mid` (viewport-sized, origin 0), then RCAS from it into the output
            write(&self.first, (0.0, 0.0));
            let size = (vw as u32, vh as u32);
            if !matches!(&self.mid, Some(m) if m.size == size) {
                let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("present mid"),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: MID_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let bind_group = self.bind_group(gpu, &texture.create_view(&Default::default()), &self.uniform);
                self.mid = Some(Mid { texture, bind_group, size });
            }
            let mid = self.mid.as_ref().unwrap();
            let mid_view = mid.texture.create_view(&Default::default());
            let mut pass = begin(encoder, &mid_view);
            pass.set_pipeline(&self.easu);
            pass.set_bind_group(0, &self.source.as_ref().unwrap().bind_group_first, &[]);
            pass.draw(0..3, 0..1);
            drop(pass);
            let mut pass = begin(encoder, view);
            pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
            pass.set_pipeline(&self.rcas);
            pass.set_bind_group(0, &mid.bind_group, &[]);
            pass.draw(0..3, 0..1);
            return;
        }
        let pipeline = match filter {
            Filter::Nearest => &self.nearest,
            Filter::Sharp => &self.sharp,
            Filter::Xbr => &self.xbr,
            Filter::Crt => &self.crt,
            Filter::Fsr => unreachable!(),
        };
        let mut pass = begin(encoder, view);
        pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.source.as_ref().unwrap().bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A render pass into `view`, cleared to black.
fn begin<'a>(encoder: &'a mut wgpu::CommandEncoder, view: &'a wgpu::TextureView) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("present"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_matches_plain_scaling_by_default() {
        assert_eq!(letterbox((1280.0, 720.0), (320.0, 200.0), false, None), (64.0, 0.0, 3.6, 3.6));
        // integer: 3x, centred
        assert_eq!(letterbox((1280.0, 720.0), (320.0, 200.0), true, None), (160.0, 60.0, 3.0, 3.0));
        // a frame larger than the output still shrinks to fit
        assert_eq!(letterbox((320.0, 200.0), (640.0, 400.0), true, None), (0.0, 0.0, 0.5, 0.5));
    }

    #[test]
    fn letterbox_with_display_aspect() {
        // 320x200 at 4:3 in 1280x720: shown 960x720 (the JS letterbox gives the same values)
        let (ox, oy, sx, sy) = letterbox((1280.0, 720.0), (320.0, 200.0), false, Some((4, 3)));
        assert_eq!((ox, oy, sx, sy), (160.0, 0.0, 3.0, 3.6));
        // integer: 3x vertically, 800x600
        assert_eq!(letterbox((1280.0, 720.0), (320.0, 200.0), true, Some((4, 3))), (240.0, 60.0, 2.5, 3.0));
    }

    #[test]
    fn filters_fall_back_when_there_is_no_room() {
        assert_eq!(effective(Filter::Xbr, 1.2), Filter::Sharp);
        assert_eq!(effective(Filter::Xbr, 3.0), Filter::Xbr);
        assert_eq!(effective(Filter::Nearest, 0.5), Filter::Sharp);
        assert_eq!(effective(Filter::Nearest, 2.5), Filter::Nearest);
        assert_eq!(effective(Filter::Fsr, 1.0), Filter::Sharp);
        assert_eq!(effective(Filter::Fsr, 1.25), Filter::Fsr);
        assert_eq!(effective(Filter::Crt, 1.9), Filter::Sharp);
        assert_eq!(effective(Filter::Crt, 2.0), Filter::Crt);
    }
}
