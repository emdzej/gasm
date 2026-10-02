// Showing video_present frames on a WebGL 2 canvas: the letterbox and the
// upscaling filters (design/presentation.md). The shaders are GLSL ports of
// runners/native/src/present.rs; keep the two in step. Display only: hashes are
// taken from the guest's bytes before any of this runs.
import { letterbox } from './lib/input.js';

/** Filter names, as `gasm-run --filter` takes them. */
export const FILTERS = ['nearest', 'sharp', 'xbr', 'fsr', 'crt'];

/** The filter that actually runs at a scale factor: shrinking is always linear
 *  (`sharp` degrades to bilinear), and xBR needs room to draw its edges. */
export function effectiveFilter(filter, scale) {
  if (scale < 1) return 'sharp';
  if (filter === 'xbr' && scale < 1.5) return 'sharp';
  if (filter === 'fsr' && scale <= 1) return 'sharp';
  if (filter === 'crt' && scale < 2) return 'sharp';
  return filter;
}

// one triangle covering the viewport
const VS = `#version 300 es
void main() {
  vec2 c = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
}`;

const HEAD = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D t;
uniform vec2 u_src;      // frame size
uniform vec2 u_out;      // the letterboxed viewport: size and bottom-left corner (whole pixels)
uniform vec2 u_origin;
out vec4 color;
// The fragment's position in frame pixels (top-left origin), from its exact
// framebuffer position; the bias sends ties the same way everywhere (present.rs).
vec2 framePos() {
  vec2 local = vec2(gl_FragCoord.x - u_origin.x, u_origin.y + u_out.y - gl_FragCoord.y);
  return local * u_src / u_out + 1.0 / 1024.0;
}
`;

const FS = {
  nearest: `${HEAD}
void main() {
  ivec2 size = textureSize(t, 0);
  color = texelFetch(t, clamp(ivec2(floor(framePos())), ivec2(0), size - 1), 0);
}`,

  // Sharp bilinear: as if the frame were first scaled by the largest whole factor
  // with nearest neighbour, then bilinearly to the output.
  sharp: `${HEAD}
void main() {
  vec2 texel = framePos();
  vec2 scale = max(floor(u_out / u_src), vec2(1.0));
  vec2 f = fract(texel) - 0.5;
  vec2 flat_half = 0.5 - 0.5 / scale + 1.0 / 512.0;  // + the bias: whole factors give exactly nearest
  vec2 g = (f - clamp(f, -flat_half, flat_half)) * scale + 0.5;
  color = texture(t, (floor(texel) + g) / u_src);
}`,

  // xBR level 2 (the algorithm of Hyllian's xBR-lv2 shaders, MIT; an independent
  // implementation). See present.rs for the description; boolean vectors are
  // 0/1 floats here (GLSL has no & and | on bvec).
  xbr: `${HEAD}
const vec3 Y = vec3(14.352, 28.176, 5.472);
const float EQ_THRESHOLD = 15.0;
const float LV2_COEFFICIENT = 2.0;
ivec2 c0;
vec3 px(int dx, int dy) {
  ivec2 size = textureSize(t, 0);
  return texelFetch(t, clamp(c0 + ivec2(dx, dy), ivec2(0), size - 1), 0).rgb;
}
vec4 df(vec4 a, vec4 b) { return abs(a - b); }
vec4 eq(vec4 a, vec4 b) { return vec4(lessThan(df(a, b), vec4(EQ_THRESHOLD))); }
vec4 neq(vec4 a, vec4 b) { return vec4(notEqual(a, b)); }
vec4 wd(vec4 a, vec4 b, vec4 c, vec4 d, vec4 e, vec4 f, vec4 g, vec4 h) {
  return df(a, b) + df(a, c) + df(d, e) + df(d, f) + 4.0 * df(g, h);
}
void main() {
  vec2 pos = framePos();
  vec2 tc = floor(pos);
  vec2 fp = pos - tc;
  c0 = ivec2(tc);
  vec3 A1 = px(-1, -2), B1 = px(0, -2), C1 = px(1, -2);
  vec3 A0 = px(-2, -1), A = px(-1, -1), B = px(0, -1), C = px(1, -1), C4 = px(2, -1);
  vec3 D0 = px(-2, 0), D = px(-1, 0), E = px(0, 0), F = px(1, 0), F4 = px(2, 0);
  vec3 G0 = px(-2, 1), G = px(-1, 1), H = px(0, 1), I = px(1, 1), I4 = px(2, 1);
  vec3 G5 = px(-1, 2), H5 = px(0, 2), I5 = px(1, 2);

  vec4 b = vec4(dot(B, Y), dot(D, Y), dot(H, Y), dot(F, Y));
  vec4 c = vec4(dot(C, Y), dot(A, Y), dot(G, Y), dot(I, Y));
  vec4 e = vec4(dot(E, Y));
  vec4 d = b.yzwx, f = b.wxyz, g = c.zwxy, h = b.zwxy, i = c.wxyz;
  vec4 i4 = vec4(dot(I4, Y), dot(C1, Y), dot(A0, Y), dot(G5, Y));
  vec4 i5 = vec4(dot(I5, Y), dot(C4, Y), dot(A1, Y), dot(G0, Y));
  vec4 h5 = vec4(dot(H5, Y), dot(F4, Y), dot(B1, Y), dot(D0, Y));
  vec4 f4 = h5.yzwx;

  vec4 fx = vec4(1.0, -1.0, -1.0, 1.0) * fp.y + vec4(1.0, 1.0, -1.0, -1.0) * fp.x;
  vec4 fx_left = vec4(1.0, -1.0, -1.0, 1.0) * fp.y + vec4(0.5, 2.0, -0.5, -2.0) * fp.x;
  vec4 fx_up = vec4(1.0, -1.0, -1.0, 1.0) * fp.y + vec4(2.0, 0.5, -2.0, -0.5) * fp.x;
  vec4 co = vec4(1.5, 0.5, -0.5, 0.5);
  vec4 cx = vec4(1.0, 1.0, -0.5, 0.0);
  vec4 cy = vec4(2.0, 0.0, -1.0, 0.5);
  float s = max(min(u_out.x / u_src.x, u_out.y / u_src.y), 1.0);
  vec4 delta = vec4(1.0 / s);
  vec4 delta_l = vec4(0.5, 1.0, 0.5, 1.0) / s;
  vec4 delta_u = delta_l.yxwz;

  vec4 lv0 = neq(e, f) * neq(e, h);
  vec4 lv1 = lv0 * clamp((1.0 - eq(f, b)) * (1.0 - eq(h, d)) + eq(e, i) * (1.0 - eq(f, i4)) * (1.0 - eq(h, i5)) + eq(e, g) + eq(e, c), 0.0, 1.0);
  vec4 lv2_left = neq(e, g) * neq(d, g);
  vec4 lv2_up = neq(e, c) * neq(b, c);
  vec4 wd1 = wd(e, c, g, i, h5, f4, h, f);
  vec4 wd2 = wd(h, d, i5, f, i4, b, e, i);
  vec4 edri = vec4(lessThanEqual(wd1, wd2)) * lv0;
  vec4 edr = vec4(lessThan(wd1, wd2)) * lv1;
  vec4 edr_left = vec4(lessThanEqual(LV2_COEFFICIENT * df(f, g), df(h, c))) * lv2_left * edr;
  vec4 edr_up = vec4(greaterThanEqual(df(f, g), LV2_COEFFICIENT * df(h, c))) * lv2_up * edr;

  vec4 fx45 = edr * clamp((fx + delta - co) / (2.0 * delta), 0.0, 1.0);
  vec4 fx45i = edri * clamp((fx + delta - co - 0.25) / (2.0 * delta), 0.0, 1.0);
  vec4 fx30 = edr_left * clamp((fx_left + delta_l - cx) / (2.0 * delta_l), 0.0, 1.0);
  vec4 fx60 = edr_up * clamp((fx_up + delta_u - cy) / (2.0 * delta_u), 0.0, 1.0);
  vec4 m = max(max(fx30, fx60), max(fx45, fx45i));
  bvec4 p = lessThanEqual(df(e, f), df(e, h));
  vec3 r1 = mix(E, p.x ? F : H, m.x);
  r1 = mix(r1, p.z ? D : B, m.z);
  vec3 r2 = mix(E, p.y ? B : F, m.y);
  r2 = mix(r2, p.w ? H : D, m.w);
  float d1 = dot(abs(E - r1), vec3(1.0));
  float d2 = dot(abs(E - r2), vec3(1.0));
  color = vec4(d2 >= d1 ? r2 : r1, 1.0);
}`,

  // AMD FSR 1, EASU pass (see present.rs): renders into the viewport-sized
  // intermediate texture, origin 0.
  easu: `${HEAD}
vec3 px(ivec2 c, int dx, int dy) {
  ivec2 size = textureSize(t, 0);
  return texelFetch(t, clamp(c + ivec2(dx, dy), ivec2(0), size - 1), 0).rgb;
}
float luma(vec3 c) { return c.b * 0.5 + (c.r * 0.5 + c.g); }
vec3 easuSet(float w, float a, float b, float c, float d, float e) {
  float lx = max(abs(d - c), abs(c - b));
  lx = lx == 0.0 ? 0.0 : 1.0 / lx;
  float dirX = d - b;
  lx = clamp(abs(dirX) * lx, 0.0, 1.0); lx = lx * lx;
  float ly = max(abs(e - c), abs(c - a));
  ly = ly == 0.0 ? 0.0 : 1.0 / ly;
  float dirY = e - a;
  ly = clamp(abs(dirY) * ly, 0.0, 1.0); ly = ly * ly;
  return vec3(dirX * w, dirY * w, (lx + ly) * w);
}
vec4 easuTap(vec2 off, vec2 dir, vec2 len2, float lob, float clp, vec3 c) {
  vec2 v = vec2(off.x * dir.x + off.y * dir.y, off.x * -dir.y + off.y * dir.x);
  v = v * len2;
  float d2 = min(v.x * v.x + v.y * v.y, clp);
  float wb = 0.4 * d2 - 1.0;
  float wa = lob * d2 - 1.0;
  wb = wb * wb; wa = wa * wa;
  wb = 1.5625 * wb - 0.5625;
  float w = wb * wa;
  return vec4(c * w, w);
}
void main() {
  vec2 pp0 = framePos() - 0.5;
  vec2 fp = floor(pp0);
  vec2 pp = pp0 - fp;
  ivec2 c0 = ivec2(fp);
  vec3 b = px(c0, 0, -1), c = px(c0, 1, -1);
  vec3 e = px(c0, -1, 0), f = px(c0, 0, 0), g = px(c0, 1, 0), h = px(c0, 2, 0);
  vec3 i = px(c0, -1, 1), j = px(c0, 0, 1), k = px(c0, 1, 1), l = px(c0, 2, 1);
  vec3 n = px(c0, 0, 2), o = px(c0, 1, 2);
  float bl = luma(b), cl = luma(c), el = luma(e), fl = luma(f), gl = luma(g), hl = luma(h);
  float il = luma(i), jl = luma(j), kl = luma(k), ll = luma(l), nl = luma(n), ol = luma(o);
  vec3 acc = easuSet((1.0 - pp.x) * (1.0 - pp.y), bl, el, fl, gl, jl);
  acc += easuSet(pp.x * (1.0 - pp.y), cl, fl, gl, hl, kl);
  acc += easuSet((1.0 - pp.x) * pp.y, fl, il, jl, kl, nl);
  acc += easuSet(pp.x * pp.y, gl, jl, kl, ll, ol);
  vec2 dir = acc.xy;
  float len = acc.z;
  float dr = dir.x * dir.x + dir.y * dir.y;
  dir = dr < 1.0 / 32768.0 ? vec2(1.0, 0.0) : dir * inversesqrt(dr);
  len = len * 0.5; len = len * len;
  float stretch = (dir.x * dir.x + dir.y * dir.y) / max(abs(dir.x), abs(dir.y));
  vec2 len2 = vec2(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
  float lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
  float clp = 1.0 / lob;
  vec4 a = vec4(0.0);
  a += easuTap(vec2(0.0, -1.0) - pp, dir, len2, lob, clp, b);
  a += easuTap(vec2(1.0, -1.0) - pp, dir, len2, lob, clp, c);
  a += easuTap(vec2(-1.0, 1.0) - pp, dir, len2, lob, clp, i);
  a += easuTap(vec2(0.0, 1.0) - pp, dir, len2, lob, clp, j);
  a += easuTap(vec2(0.0, 0.0) - pp, dir, len2, lob, clp, f);
  a += easuTap(vec2(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
  a += easuTap(vec2(1.0, 1.0) - pp, dir, len2, lob, clp, k);
  a += easuTap(vec2(2.0, 1.0) - pp, dir, len2, lob, clp, l);
  a += easuTap(vec2(2.0, 0.0) - pp, dir, len2, lob, clp, h);
  a += easuTap(vec2(1.0, 0.0) - pp, dir, len2, lob, clp, g);
  a += easuTap(vec2(1.0, 2.0) - pp, dir, len2, lob, clp, o);
  a += easuTap(vec2(0.0, 2.0) - pp, dir, len2, lob, clp, n);
  vec3 mn = min(min(f, g), min(j, k));
  vec3 mx = max(max(f, g), max(j, k));
  color = vec4(min(mx, max(mn, a.rgb / a.w)), 1.0);
}`,

  // CRT (see present.rs): two nearest lines as beams, aperture grille, in linear light.
  crt: `${HEAD}
const float CRT_MASK = 0.3;
const float CRT_BOOST = 1.45;
vec3 crtLine(float x, float line) {
  vec3 c = textureLod(t, vec2(x, line + 0.5) / u_src, 0.0).rgb;
  return pow(c, vec3(2.4));
}
vec3 crtBeam(float d, vec3 c) {
  vec3 width = mix(vec3(0.3), vec3(0.55), sqrt(c));
  vec3 e = d / width;
  return exp(-e * e);
}
void main() {
  vec2 pos = framePos();
  float sx = max(floor(u_out.x / u_src.x), 1.0);
  float f = fract(pos.x) - 0.5;
  float flatHalf = 0.5 - 0.5 / sx + 1.0 / 512.0;
  float x = floor(pos.x) + (f - clamp(f, -flatHalf, flatHalf)) * sx + 0.5;
  float y = pos.y - 0.5;
  float line = floor(y);
  float d = y - line;
  vec3 c0 = crtLine(x, line);
  vec3 c1 = crtLine(x, min(line + 1.0, u_src.y - 1.0));
  vec3 col = c0 * crtBeam(d, c0) + c1 * crtBeam(1.0 - d, c1);
  uint stripe = uint(floor(gl_FragCoord.x)) % 3u;
  vec3 mask = mix(vec3(1.0 - CRT_MASK), vec3(1.0), vec3(equal(uvec3(0u, 1u, 2u), uvec3(stripe))));
  col = col * mask * CRT_BOOST;
  color = vec4(pow(min(col, vec3(1.0)), vec3(1.0 / 2.4)), 1.0);
}`,

  // FSR RCAS pass: sharpens the intermediate texture into the output. Rows of a
  // framebuffer texture run bottom-up; the 5-tap cross is symmetric, so the
  // texel is simply the fragment's position in the viewport.
  rcas: `${HEAD}
const float RCAS_SHARPNESS = 0.87055056;
const float RCAS_LIMIT = 0.1875;
void main() {
  ivec2 c0 = ivec2(floor(gl_FragCoord.xy - u_origin));
  ivec2 size = textureSize(t, 0) - 1;
  vec3 b = texelFetch(t, clamp(c0 + ivec2(0, 1), ivec2(0), size), 0).rgb;
  vec3 d = texelFetch(t, clamp(c0 + ivec2(-1, 0), ivec2(0), size), 0).rgb;
  vec3 e = texelFetch(t, clamp(c0, ivec2(0), size), 0).rgb;
  vec3 f = texelFetch(t, clamp(c0 + ivec2(1, 0), ivec2(0), size), 0).rgb;
  vec3 h = texelFetch(t, clamp(c0 + ivec2(0, -1), ivec2(0), size), 0).rgb;
  vec3 mn = min(min(b, d), min(f, h));
  vec3 mx = max(max(b, d), max(f, h));
  vec3 hitMin = mn / max(4.0 * mx, vec3(1.0 / 1024.0));
  vec3 hitMax = (1.0 - mx) / min(4.0 * mn - 4.0, vec3(-1.0 / 1024.0));
  vec3 lobeRgb = max(-hitMin, hitMax);
  float lobe = max(-RCAS_LIMIT, min(max(lobeRgb.r, max(lobeRgb.g, lobeRgb.b)), 0.0)) * RCAS_SHARPNESS;
  color = vec4((lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0), 1.0);
}`
};

/**
 * Draws 2D frames onto a canvas with WebGL 2. The canvas is resized to the
 * output size given to draw() (its display size in device pixels).
 *   const p = GlPresenter.create(canvas);   // null without WebGL 2
 *   p.draw(rgba, w, h, [displayW, displayH], { filter: 'xbr', integerScale: false });
 */
export class GlPresenter {
  static create(canvas) {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, preserveDrawingBuffer: false });
    return gl ? new GlPresenter(gl) : null;
  }

  constructor(gl) {
    this.gl = gl; this.canvas = gl.canvas;
    this.programs = {};
    this.vao = gl.createVertexArray();
    this.texture = gl.createTexture();
    this.size = [0, 0];
  }

  program(filter) {
    if (this.programs[filter]) return this.programs[filter];
    const gl = this.gl;
    const shader = (type, src) => {
      const s = gl.createShader(type);
      gl.shaderSource(s, src); gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`present ${filter}: ${gl.getShaderInfoLog(s)}`);
      return s;
    };
    const p = gl.createProgram();
    gl.attachShader(p, shader(gl.VERTEX_SHADER, VS));
    gl.attachShader(p, shader(gl.FRAGMENT_SHADER, FS[filter]));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`present ${filter}: ${gl.getProgramInfoLog(p)}`);
    const loc = (n) => gl.getUniformLocation(p, n);
    const prog = { p, src: loc('u_src'), out: loc('u_out'), origin: loc('u_origin'), t: loc('t') };
    this.programs[filter] = prog;
    return prog;
  }

  /** Draw a w×h RGBA frame letterboxed into an output of `size` device pixels. */
  draw(rgba, w, h, [ow, oh], { filter = 'sharp', integerScale = false, aspect = null } = {}) {
    const gl = this.gl;
    if (this.canvas.width !== ow || this.canvas.height !== oh) { this.canvas.width = ow; this.canvas.height = oh; }
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const px = rgba instanceof Uint8Array ? rgba : new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength);
    if (this.size[0] !== w || this.size[1] !== h) {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, px);
      this.size = [w, h];
    } else {
      gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    }
    const [ox, oy, sx, sy] = letterbox([ow, oh], [w, h], integerScale, aspect);
    const eff = effectiveFilter(filter, Math.min(sx, sy));
    const lin = eff === 'sharp' || eff === 'crt' ? gl.LINEAR : gl.NEAREST;
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, lin);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, lin);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.disable(gl.SCISSOR_TEST);
    gl.viewport(0, 0, ow, oh);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    // WebGL viewports are whole pixels (natively they can be fractional)
    const vx = Math.round(ox), vy = Math.round(oy), vw = Math.round(w * sx), vh = Math.round(h * sy);
    gl.viewport(vx, oh - vy - vh, vw, vh);
    gl.bindVertexArray(this.vao);
    if (eff === 'fsr') {
      // EASU into the viewport-sized intermediate texture, then RCAS from it into the canvas
      const mid = this.midTarget(vw, vh);
      gl.bindFramebuffer(gl.FRAMEBUFFER, mid.fb);
      gl.viewport(0, 0, vw, vh);
      this.pass('easu', w, h, vw, vh, 0, 0);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(vx, oh - vy - vh, vw, vh);
      gl.bindTexture(gl.TEXTURE_2D, mid.tex);
      this.pass('rcas', w, h, vw, vh, vx, oh - vy - vh);
      return;
    }
    this.pass(eff, w, h, vw, vh, vx, oh - vy - vh);
  }

  /** One full-viewport pass of `filter` from the bound texture. */
  pass(filter, w, h, vw, vh, ox, oy) {
    const gl = this.gl, prog = this.program(filter);
    gl.useProgram(prog.p);
    gl.uniform1i(prog.t, 0);
    gl.uniform2f(prog.src, w, h);
    gl.uniform2f(prog.out, vw, vh);
    gl.uniform2f(prog.origin, ox, oy);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /** The intermediate image of two-pass filters, one texel per viewport pixel. */
  midTarget(w, h) {
    const gl = this.gl;
    if (this.mid?.w === w && this.mid?.h === h) return this.mid;
    if (this.mid) { gl.deleteFramebuffer(this.mid.fb); gl.deleteTexture(this.mid.tex); }
    const tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, w, h);
    for (const [k, v] of [[gl.TEXTURE_MIN_FILTER, gl.NEAREST], [gl.TEXTURE_MAG_FILTER, gl.NEAREST]]) gl.texParameteri(gl.TEXTURE_2D, k, v);
    const fb = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    this.mid = { w, h, tex, fb };
    return this.mid;
  }

  /** The canvas as tightly packed RGBA rows, top to bottom (call right after draw; tests). */
  read() {
    const gl = this.gl, w = this.canvas.width, h = this.canvas.height;
    const px = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    const out = new Uint8Array(px.length), row = w * 4;
    for (let y = 0; y < h; y++) out.set(px.subarray((h - 1 - y) * row, (h - y) * row), y * row);
    return out;
  }
}
