//! The gasm splash screen: a few frames of Pong in the style of gasm's icon (the
//! paddles, the dashed net and the square ball on the dark tile), then the court
//! shrinks into the icon, the paddles and the ball move into their places in it,
//! and "gasm" appears under it.
//!
//! Frames are 2D (RGBA8, [`W`]×[`H`]) and shown through the runner's usual 2D
//! presentation (letterbox, filter). Everything is integer arithmetic, so
//! runners/web/lib/splash.js draws the same pixels (`splash_hash` in the tests
//! and scripts/splash-test.mjs compare them).

pub const W: usize = 320;
pub const H: usize = 180;
/// Frames at 60 Hz: the rally, the move into the logo, the name, a pause, the fade.
pub const FRAMES: u32 = 96;
const RALLY_END: u32 = 36;
const MORPH_END: u32 = 60;
const NAME_END: u32 = 72;
const FADE_START: u32 = 84;
/// The logo with the name: runners stay on it while the game is still loading.
pub const HOLD: u32 = FADE_START - 1;

const BG: [u8; 3] = [0x07, 0x09, 0x0c];
const TILE: [u8; 3] = [0x0e, 0x11, 0x16];
const NET: [u8; 3] = [0x3a, 0x42, 0x50];
const RED: [u8; 3] = [0xe8, 0x55, 0x4e];
const BLUE: [u8; 3] = [0x3b, 0x7f, 0xf5];
const WHITE: [u8; 3] = [0xf2, 0xf4, 0xf8];

/// A rectangle: x, y, w, h.
type Rect = [i32; 4];

// The logo: the icon (a 64-unit viewBox) at 2x, its tile at (LOGO_X, LOGO_Y).
const LOGO_X: i32 = (W as i32 - 128) / 2;
const LOGO_Y: i32 = 10;
const fn logo(x: i32, y: i32, w: i32, h: i32) -> Rect {
    [LOGO_X + x * 2, LOGO_Y + y * 2, w * 2, h * 2]
}
const LOGO_TILE: Rect = logo(2, 2, 60, 60);
const LOGO_RADIUS: i32 = 26;
const LOGO_LEFT: Rect = logo(9, 15, 5, 18);
const LOGO_RIGHT: Rect = logo(50, 29, 5, 18);
const LOGO_BALL: Rect = logo(38, 22, 6, 6);
/// the net's dashes in the icon: x 30.5, width 3, every 9 units from 9
const fn logo_dash(i: i32) -> Rect {
    [LOGO_X + 61, LOGO_Y + (9 + 9 * i) * 2, 6, 10]
}

// The court: the whole frame.
const COURT: Rect = [0, 0, W as i32, H as i32];
const PADDLE_W: i32 = 10;
const PADDLE_H: i32 = 36;
const LEFT_X: i32 = 18;
const RIGHT_X: i32 = W as i32 - 18 - PADDLE_W;
const BALL: i32 = 12;
const fn court_dash(i: i32) -> Rect {
    [W as i32 / 2 - 3, 8 + 36 * i, 6, 20]
}

// The ball's left edge travels between the paddles' faces, LEFT_FACE..RIGHT_FACE, at
// VX px per frame: one crossing every CROSS frames, so the paddles meet it exactly.
const LEFT_FACE: i32 = LEFT_X + PADDLE_W;
const RIGHT_FACE: i32 = RIGHT_X - BALL;
const VX: i32 = 9;
const CROSS: i32 = (RIGHT_FACE - LEFT_FACE) / VX; // 28
/// The right paddle hits at frame RIGHT_HIT, the left one CROSS frames later (the
/// rally's last frame), and so on every 2 * CROSS.
const RIGHT_HIT: i32 = RALLY_END as i32 - CROSS;
const VY: i32 = 5;

fn bounce(p: i32, lo: i32, hi: i32) -> i32 {
    let span = hi - lo;
    let m = (p - lo).rem_euclid(2 * span);
    lo + if m <= span { m } else { 2 * span - m }
}

/// The ball during the rally (frame t, any integer).
fn ball_at(t: i32) -> (i32, i32) {
    (bounce(RIGHT_FACE - (RIGHT_HIT - t) * VX, LEFT_FACE, RIGHT_FACE), bounce(40 + t * VY, 0, H as i32 - BALL))
}

/// A paddle's top at frame t: it moves evenly from where it met the ball last to
/// where it will meet it next (hits every 2 * CROSS frames from `first`), meeting
/// it a little off center.
fn paddle_y(t: i32, first: i32, offset: i32) -> i32 {
    let period = 2 * CROSS;
    let prev = first + (t - first).div_euclid(period) * period;
    let next = prev + period;
    let at = |h: i32| ball_at(h).1 + BALL / 2 - PADDLE_H / 2 + offset;
    let y = at(prev) + (at(next) - at(prev)) * (t - prev) / period;
    y.clamp(0, H as i32 - PADDLE_H)
}

/// Smoothstep over 0..=n in 1/4096ths: 3t² - 2t³.
fn ease(t: u32, n: u32) -> i64 {
    let t = t.min(n) as i64;
    let n = n as i64;
    (4096 * (3 * t * t * n - 2 * t * t * t)) / (n * n * n)
}

fn lerp(a: i32, b: i32, e: i64) -> i32 {
    a + ((b - a) as i64 * e / 4096) as i32
}

fn lerp_rect(a: Rect, b: Rect, e: i64) -> Rect {
    [lerp(a[0], b[0], e), lerp(a[1], b[1], e), lerp(a[2], b[2], e), lerp(a[3], b[3], e)]
}

fn put(px: &mut [u8], x: i32, y: i32, c: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
        let i = (y as usize * W + x as usize) * 4;
        px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
    }
}

fn fill(px: &mut [u8], r: Rect, c: [u8; 3]) {
    for y in r[1]..r[1] + r[3] {
        for x in r[0]..r[0] + r[2] {
            put(px, x, y, c);
        }
    }
}

/// A rectangle with rounded corners: a pixel is in if its center is (in half pixels).
fn fill_round(px: &mut [u8], r: Rect, radius: i32, c: [u8; 3]) {
    let rad = radius.min(r[2] / 2).min(r[3] / 2);
    for y in r[1]..r[1] + r[3] {
        for x in r[0]..r[0] + r[2] {
            // distance from the nearest corner circle's center, doubled
            let cx = (2 * x + 1).clamp(2 * (r[0] + rad), 2 * (r[0] + r[2] - rad));
            let cy = (2 * y + 1).clamp(2 * (r[1] + rad), 2 * (r[1] + r[3] - rad));
            let (dx, dy) = (2 * x + 1 - cx, 2 * y + 1 - cy);
            if dx * dx + dy * dy <= 4 * rad * rad {
                put(px, x, y, c);
            }
        }
    }
}

// "gasm" in a 5-wide pixel font, x-height rows 0..5, g's descender to row 7.
const GLYPHS: [[&str; 7]; 4] = [
    [".####", "#...#", "#...#", ".####", "....#", "....#", ".###."],
    [".###.", "....#", ".####", "#...#", ".####", ".....", "....."],
    [".####", "#....", ".###.", "....#", "####.", ".....", "....."],
    ["##.#.", "#.#.#", "#.#.#", "#.#.#", "#.#.#", ".....", "....."],
];
const SCALE: i32 = 3;
const NAME_Y: i32 = LOGO_Y + 128 + 12;
const NAME_X: i32 = (W as i32 - (4 * 5 + 3 * 1) * SCALE) / 2;

fn name(px: &mut [u8], letters: usize) {
    for (g, glyph) in GLYPHS.iter().enumerate().take(letters) {
        for (row, bits) in glyph.iter().enumerate() {
            for (col, b) in bits.bytes().enumerate() {
                if b == b'#' {
                    let x = NAME_X + (g as i32 * 6 + col as i32) * SCALE;
                    fill(px, [x, NAME_Y + row as i32 * SCALE, SCALE, SCALE], WHITE);
                }
            }
        }
    }
}

/// Frame `f` (0..[`FRAMES`]; later frames are the last one) as RGBA8, `W`×`H`.
pub fn frame(f: u32) -> Vec<u8> {
    let f = f.min(FRAMES - 1);
    let mut px = vec![0u8; W * H * 4];
    fill(&mut px, COURT, BG);
    // where everything is when the rally ends, and how far it has moved since
    let r = f.min(RALLY_END);
    let e = if f <= RALLY_END { 0 } else { ease(f - RALLY_END, MORPH_END - RALLY_END) };
    let (bx, by) = ball_at(r as i32);
    let tile = lerp_rect(COURT, LOGO_TILE, e);
    fill_round(&mut px, tile, lerp(0, LOGO_RADIUS, e), TILE);
    for i in 0..5 {
        fill(&mut px, lerp_rect(court_dash(i), logo_dash(i), e), NET);
    }
    fill(&mut px, lerp_rect([LEFT_X, paddle_y(r as i32, RIGHT_HIT + CROSS, -7), PADDLE_W, PADDLE_H], LOGO_LEFT, e), RED);
    fill(&mut px, lerp_rect([RIGHT_X, paddle_y(r as i32, RIGHT_HIT, 8), PADDLE_W, PADDLE_H], LOGO_RIGHT, e), BLUE);
    fill(&mut px, lerp_rect([bx, by, BALL, BALL], LOGO_BALL, e), WHITE);
    if f > MORPH_END {
        let letters = 1 + (f - MORPH_END) as usize * 4 / (NAME_END - MORPH_END) as usize;
        name(&mut px, letters.min(4));
    }
    if f >= FADE_START {
        // to black, in whole steps of 1/16
        let k = 16 - (16 * (f - FADE_START + 1) / (FRAMES - FADE_START)).min(16);
        for p in px.chunks_exact_mut(4) {
            for c in &mut p[..3] {
                *c = (*c as u32 * k / 16) as u8;
            }
        }
    }
    px
}

/// [`splash_hash`] of these frames: change it with the animation (the unit test says
/// the new value); `gen-abi.mjs --check` compares runners/web/lib/splash.js with it.
pub const SPLASH_HASH: u32 = 0xab6bc49a;

/// FNV-1a 32 over every frame: the same as runners/web/lib/splash.js.
pub fn splash_hash() -> u32 {
    (0..FRAMES).flat_map(frame).fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        // SPLASH_DIR=dir cargo test --lib splash: write every frame as a PNG (to look at)
        if let Ok(dir) = std::env::var("SPLASH_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            for f in 0..FRAMES {
                let file = std::fs::File::create(format!("{dir}/{f:03}.png")).unwrap();
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
                enc.set_color(png::ColorType::Rgba);
                enc.write_header().unwrap().write_image_data(&frame(f)).unwrap();
            }
        }
        // the logo is the icon: its ball sits where the icon's does
        let last = frame(MORPH_END);
        let i = ((LOGO_BALL[1] + 1) as usize * W + (LOGO_BALL[0] + 1) as usize) * 4;
        assert_eq!(&last[i..i + 3], &WHITE);
        assert_eq!(splash_hash(), SPLASH_HASH, "the frames changed: SPLASH_HASH = {:#010x}", splash_hash());
    }
}
