// The gasm splash screen, a port of runners/native/src/splash.rs: a few frames of Pong
// in the style of gasm's icon, the court shrinking into the icon, "gasm" under it.
// Integer arithmetic only, so both runners draw the same pixels (splashHash() ==
// splash::splash_hash(), checked by scripts/splash-test.mjs). Frames are 2D RGBA8,
// SPLASH_W x SPLASH_H, shown like video_present frames.

export const SPLASH_W = 320, SPLASH_H = 180;
export const SPLASH_FRAMES = 96;
const W = SPLASH_W, H = SPLASH_H;
const RALLY_END = 36, MORPH_END = 60, NAME_END = 72, FADE_START = 84;
/** The logo with the name: runners stay on it while the game is still loading. */
export const SPLASH_HOLD = FADE_START - 1;

const BG = [0x07, 0x09, 0x0c], TILE = [0x0e, 0x11, 0x16], NET = [0x3a, 0x42, 0x50];
const RED = [0xe8, 0x55, 0x4e], BLUE = [0x3b, 0x7f, 0xf5], WHITE = [0xf2, 0xf4, 0xf8];

const LOGO_X = (W - 128) / 2, LOGO_Y = 10;
const logo = (x, y, w, h) => [LOGO_X + x * 2, LOGO_Y + y * 2, w * 2, h * 2];
const LOGO_TILE = logo(2, 2, 60, 60), LOGO_RADIUS = 26;
const LOGO_LEFT = logo(9, 15, 5, 18), LOGO_RIGHT = logo(50, 29, 5, 18), LOGO_BALL = logo(38, 22, 6, 6);
const logoDash = (i) => [LOGO_X + 61, LOGO_Y + (9 + 9 * i) * 2, 6, 10];

const COURT = [0, 0, W, H];
const PADDLE_W = 10, PADDLE_H = 36, LEFT_X = 18, RIGHT_X = W - 18 - PADDLE_W, BALL = 12;
const courtDash = (i) => [W / 2 - 3, 8 + 36 * i, 6, 20];

const LEFT_FACE = LEFT_X + PADDLE_W, RIGHT_FACE = RIGHT_X - BALL, VX = 9;
const CROSS = Math.trunc((RIGHT_FACE - LEFT_FACE) / VX);
const RIGHT_HIT = RALLY_END - CROSS, VY = 5;

const mod = (a, n) => ((a % n) + n) % n;
const div = (a, n) => Math.floor(a / n);   // Rust's div_euclid for n > 0
const tdiv = (a, n) => Math.trunc(a / n);  // Rust's / on integers
const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);

function bounce(p, lo, hi) {
  const span = hi - lo, m = mod(p - lo, 2 * span);
  return lo + (m <= span ? m : 2 * span - m);
}
const ballAt = (t) => [bounce(RIGHT_FACE - (RIGHT_HIT - t) * VX, LEFT_FACE, RIGHT_FACE), bounce(40 + t * VY, 0, H - BALL)];

function paddleY(t, first, offset) {
  const period = 2 * CROSS;
  const prev = first + div(t - first, period) * period, next = prev + period;
  const at = (h) => ballAt(h)[1] + BALL / 2 - PADDLE_H / 2 + offset;
  return clamp(at(prev) + tdiv((at(next) - at(prev)) * (t - prev), period), 0, H - PADDLE_H);
}

// smoothstep over 0..n in 1/4096ths (BigInt: n³·4096 and the products stay exact)
function ease(t, n) {
  const T = BigInt(Math.min(t, n)), N = BigInt(n);
  return Number((4096n * (3n * T * T * N - 2n * T * T * T)) / (N * N * N));
}
const lerp = (a, b, e) => a + tdiv((b - a) * e, 4096);
const lerpRect = (a, b, e) => a.map((v, i) => lerp(v, b[i], e));

function put(px, x, y, c) {
  if (x >= 0 && y >= 0 && x < W && y < H) {
    const i = (y * W + x) * 4;
    px[i] = c[0]; px[i + 1] = c[1]; px[i + 2] = c[2]; px[i + 3] = 255;
  }
}
function fill(px, r, c) {
  for (let y = r[1]; y < r[1] + r[3]; y++) for (let x = r[0]; x < r[0] + r[2]; x++) put(px, x, y, c);
}
function fillRound(px, r, radius, c) {
  const rad = Math.min(radius, tdiv(r[2], 2), tdiv(r[3], 2));
  for (let y = r[1]; y < r[1] + r[3]; y++) {
    for (let x = r[0]; x < r[0] + r[2]; x++) {
      const cx = clamp(2 * x + 1, 2 * (r[0] + rad), 2 * (r[0] + r[2] - rad));
      const cy = clamp(2 * y + 1, 2 * (r[1] + rad), 2 * (r[1] + r[3] - rad));
      const dx = 2 * x + 1 - cx, dy = 2 * y + 1 - cy;
      if (dx * dx + dy * dy <= 4 * rad * rad) put(px, x, y, c);
    }
  }
}

const GLYPHS = [
  ['.####', '#...#', '#...#', '.####', '....#', '....#', '.###.'],
  ['.###.', '....#', '.####', '#...#', '.####', '.....', '.....'],
  ['.####', '#....', '.###.', '....#', '####.', '.....', '.....'],
  ['##.#.', '#.#.#', '#.#.#', '#.#.#', '#.#.#', '.....', '.....'],
];
const SCALE = 3, NAME_Y = LOGO_Y + 128 + 12, NAME_X = tdiv(W - (4 * 5 + 3) * SCALE, 2);
function name(px, letters) {
  GLYPHS.slice(0, letters).forEach((glyph, g) => glyph.forEach((bits, row) => [...bits].forEach((b, col) => {
    if (b === '#') fill(px, [NAME_X + (g * 6 + col) * SCALE, NAME_Y + row * SCALE, SCALE, SCALE], WHITE);
  })));
}

/** Frame f (0..SPLASH_FRAMES-1; later frames are the last one) as RGBA8, SPLASH_W x SPLASH_H. */
export function splashFrame(f) {
  f = Math.min(f, SPLASH_FRAMES - 1);
  const px = new Uint8ClampedArray(W * H * 4);
  fill(px, COURT, BG);
  const r = Math.min(f, RALLY_END);
  const e = f <= RALLY_END ? 0 : ease(f - RALLY_END, MORPH_END - RALLY_END);
  const [bx, by] = ballAt(r);
  fillRound(px, lerpRect(COURT, LOGO_TILE, e), lerp(0, LOGO_RADIUS, e), TILE);
  for (let i = 0; i < 5; i++) fill(px, lerpRect(courtDash(i), logoDash(i), e), NET);
  fill(px, lerpRect([LEFT_X, paddleY(r, RIGHT_HIT + CROSS, -7), PADDLE_W, PADDLE_H], LOGO_LEFT, e), RED);
  fill(px, lerpRect([RIGHT_X, paddleY(r, RIGHT_HIT, 8), PADDLE_W, PADDLE_H], LOGO_RIGHT, e), BLUE);
  fill(px, lerpRect([bx, by, BALL, BALL], LOGO_BALL, e), WHITE);
  if (f > MORPH_END) name(px, Math.min(4, 1 + tdiv((f - MORPH_END) * 4, NAME_END - MORPH_END)));
  if (f >= FADE_START) {
    const k = 16 - Math.min(16, tdiv(16 * (f - FADE_START + 1), SPLASH_FRAMES - FADE_START));
    for (let i = 0; i < px.length; i += 4) for (let c = 0; c < 3; c++) px[i + c] = tdiv(px[i + c] * k, 16);
  }
  return px;
}

/** FNV-1a 32 over every frame (as splash::splash_hash natively). */
export function splashHash() {
  let h = 0x811c9dc5;
  for (let f = 0; f < SPLASH_FRAMES; f++) for (const b of splashFrame(f)) h = Math.imul(h ^ b, 0x01000193) >>> 0;
  return h;
}
