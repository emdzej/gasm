//! A brick breaker that plays itself, in gasm's colours: the website's background.
//! A wall of bricks across the top, a paddle that aims for what's left, a ball with a
//! trail, bricks that burst into pieces; a cleared wall drops in the next pattern.
//!
//! Params `w`, `h`: the frame size (default 640×360), so the picture has the page's
//! shape; everything scales with the height. `fg`: the ball's and paddle's colour as
//! RRGGBB hex (default f2f4f8, light on dark; a light page passes a dark one). 2D frames (RGBA) with a transparent
//! background, so the page's light and dark colours show through. Only
//! `+ - * / sqrt` on floats: the same frames on every runner.

const COLORS: [[u8; 3]; 6] = [
    [0xe8, 0x55, 0x4e], // the icon's red paddle
    [0xf0, 0x7a, 0x9a], // coral pink
    [0xf2, 0xb8, 0x4b], // amber
    [0x88, 0xc0, 0xd0], // the site's teal
    [0x3b, 0x7f, 0xf5], // the icon's blue paddle
    [0xa7, 0x8b, 0xfa], // violet
];
const ROWS: usize = 6;
const TRAIL: usize = 8;

struct Piece {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: u32,
    color: [u8; 3],
}

struct Bricks {
    w: usize,
    h: usize,
    cols: usize,
    brick_w: f32,
    brick_h: f32,
    gap: f32,
    left: f32,
    top: f32,
    /// row-major; false: broken (or not in this pattern)
    alive: Vec<bool>,
    pattern: usize,
    /// frames since the wall appeared (it drops in)
    wall_age: u32,
    paddle_x: f32,
    paddle_w: f32,
    paddle_h: f32,
    paddle_y: f32,
    /// where on the paddle the next hit lands (-1..1): aims the ball
    aim: f32,
    ball: f32,
    bx: f32,
    by: f32,
    vx: f32,
    vy: f32,
    speed: f32,
    trail: [(f32, f32); TRAIL],
    pieces: Vec<Piece>,
    rng: u32,
    /// the ball's and paddle's colour
    fg: [u8; 3],
    fb: Vec<u8>,
}

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo { lo } else if v > hi { hi } else { v }
}

impl Bricks {
    fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// The next wall: a pattern of which bricks exist.
    fn new_wall(&mut self) {
        let (rows, cols) = (ROWS, self.cols);
        let p = self.pattern % 5;
        self.pattern += 1;
        let mid = (cols as f32 - 1.0) / 2.0;
        for r in 0..rows {
            for c in 0..cols {
                let d = if (c as f32) < mid { mid - c as f32 } else { c as f32 - mid };
                self.alive[r * cols + c] = match p {
                    0 => true,                                       // full
                    1 => (r + c) % 2 == 0,                           // checkerboard
                    2 => d <= (rows - r) as f32 * mid / rows as f32 + 0.5, // pyramid
                    3 => d + (r as f32 - 2.5).abs() * mid / 3.0 <= mid,     // diamond
                    _ => r % 2 == 0 || c % 3 != 1,                   // stripes with gaps
                };
            }
        }
        self.wall_age = 0;
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [u8; 3], a: f32) {
        let (fw, fh) = (self.w as i32, self.h as i32);
        let x0 = clamp(x, 0.0, fw as f32) as i32;
        let x1 = clamp(x + w, 0.0, fw as f32) as i32;
        let y0 = clamp(y, 0.0, fh as f32) as i32;
        let y1 = clamp(y + h, 0.0, fh as f32) as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c, a);
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, c: [u8; 3], a: f32) {
        let i = ((y * self.w as i32 + x) * 4) as usize;
        let under = self.fb[i + 3] as f32 / 255.0;
        let out = a + under * (1.0 - a);
        if out <= 0.0 {
            return;
        }
        for k in 0..3 {
            self.fb[i + k] = ((c[k] as f32 * a + self.fb[i + k] as f32 * under * (1.0 - a)) / out) as u8;
        }
        self.fb[i + 3] = (out * 255.0) as u8;
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: [u8; 3], alpha: f32) {
        let (fw, fh) = (self.w as f32, self.h as f32);
        let (x0, x1) = (clamp(cx - r - 1.0, 0.0, fw) as i32, clamp(cx + r + 2.0, 0.0, fw) as i32);
        let (y0, y1) = (clamp(cy - r - 1.0, 0.0, fh) as i32, clamp(cy + r + 2.0, 0.0, fh) as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let a = clamp(r - (dx * dx + dy * dy).sqrt() + 0.5, 0.0, 1.0) * alpha;
                if a > 0.0 {
                    self.blend(x, y, c, a);
                }
            }
        }
    }

    fn lighter(c: [u8; 3], k: f32) -> [u8; 3] {
        [0, 1, 2].map(|i| (c[i] as f32 + (255.0 - c[i] as f32) * k) as u8)
    }

    fn darker(c: [u8; 3], k: f32) -> [u8; 3] {
        [0, 1, 2].map(|i| (c[i] as f32 * (1.0 - k)) as u8)
    }

    /// How far the wall has dropped in (0..1, eased).
    fn drop(&self) -> f32 {
        let t = clamp(self.wall_age as f32 / 45.0, 0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn brick_rect(&self, r: usize, c: usize) -> (f32, f32) {
        let x = self.left + c as f32 * (self.brick_w + self.gap);
        let rise = (1.0 - self.drop()) * (self.top + ROWS as f32 * (self.brick_h + self.gap));
        let y = self.top + r as f32 * (self.brick_h + self.gap) - rise;
        (x, y)
    }
}

impl gasm::Game for Bricks {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let num = |k: &str, d: usize, lo: usize, hi: usize| gasm::param(k).and_then(|v| v.parse().ok()).unwrap_or(d).clamp(lo, hi);
        let (w, h) = (num("w", 640, 160, 1600), num("h", 360, 90, 720));
        let (wf, hf) = (w as f32, h as f32);
        let gap = (hf / 180.0).max(1.0) as i32 as f32;
        let brick_h = (hf * 0.04) as i32 as f32;
        let target_w = hf * 0.11;
        let cols = ((wf - 2.0 * gap) / (target_w + gap)) as usize;
        let brick_w = ((wf - gap * (cols as f32 + 1.0)) / cols as f32) as i32 as f32;
        let left = ((wf - (cols as f32 * (brick_w + gap) - gap)) / 2.0) as i32 as f32;
        let speed = hf / 70.0;
        let fg = gasm::param("fg")
            .and_then(|v| u32::from_str_radix(v.trim_start_matches('#'), 16).ok())
            .map_or([0xf2, 0xf4, 0xf8], |c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        let paddle_w = hf * 0.16;
        let mut g = Bricks {
            w, h, cols, brick_w, brick_h, gap, left, top: hf * 0.1,
            alive: vec![false; ROWS * cols], pattern: 0, wall_age: 0,
            paddle_x: wf / 2.0 - paddle_w / 2.0, paddle_w, paddle_h: (hf * 0.025).max(2.0), paddle_y: hf * 0.88,
            aim: 0.3, ball: (hf * 0.014).max(1.5),
            bx: wf / 2.0, by: hf * 0.6, vx: speed * 0.5, vy: speed * 0.85, speed,
            trail: [(wf / 2.0, hf * 0.6); TRAIL], pieces: Vec::new(), rng: 11, fg,
            fb: vec![0; w * h * 4],
        };
        g.new_wall();
        g.wall_age = 45;
        Ok(g)
    }

    fn frame(&mut self) {
        let (wf, hf) = (self.w as f32, self.h as f32);
        self.wall_age += 1;
        // the paddle: under where the ball will come down, offset so the hit sends it
        // where the aim says (towards the bricks that are left, varied by chance)
        let target = if self.vy > 0.0 {
            let t = (self.paddle_y - self.by) / self.vy;
            let mut x = self.bx + self.vx * t;
            // fold the predicted x back into the court (walls reflect)
            let span = wf - 2.0 * self.ball;
            let mut m = x - self.ball;
            let period = 2.0 * span;
            m -= (m / period) as i32 as f32 * period;
            if m < 0.0 { m += period; }
            x = self.ball + if m <= span { m } else { period - m };
            x - self.paddle_w / 2.0 - self.aim * self.paddle_w / 2.0
        } else {
            wf / 2.0 - self.paddle_w / 2.0
        };
        let step = self.speed * 1.2;
        self.paddle_x = clamp(self.paddle_x + clamp(target - self.paddle_x, -step, step), 0.0, wf - self.paddle_w);

        // the ball
        self.trail.rotate_right(1);
        self.trail[0] = (self.bx, self.by);
        self.bx += self.vx;
        self.by += self.vy;
        let r = self.ball;
        if self.bx < r || self.bx > wf - r {
            self.vx = -self.vx;
            self.bx = clamp(self.bx, r, wf - r);
        }
        if self.by < r {
            self.vy = -self.vy;
            self.by = r;
        }
        // the paddle: the angle depends on where it hits (the edges send it sideways)
        if self.vy > 0.0 && self.by + r >= self.paddle_y && self.by - r <= self.paddle_y + self.paddle_h
            && self.bx >= self.paddle_x - r && self.bx <= self.paddle_x + self.paddle_w + r
        {
            // off towards a brick that's left (chosen at random, lower rows likelier), at
            // least a little sideways so it never goes straight up and down
            let alive: Vec<usize> = (0..self.alive.len()).filter(|&i| self.alive[i]).collect();
            let (mut ux, mut uy) = (0.3, -1.0);
            if !alive.is_empty() {
                let pick = (self.random() * self.random() * alive.len() as f32) as usize;
                let i = alive[alive.len() - 1 - pick.min(alive.len() - 1)];   // from the bottom rows up
                let (x, y) = self.brick_rect(i / self.cols, i % self.cols);
                (ux, uy) = (x + self.brick_w / 2.0 - self.bx, y + self.brick_h / 2.0 - self.paddle_y);
            }
            let len = (ux * ux + uy * uy).sqrt();
            let (mut dx, dy) = (ux / len, uy / len);
            if dx > -0.25 && dx < 0.25 {
                dx = if dx < 0.0 { -0.25 } else { 0.25 };
            }
            let n = (dx * dx + dy * dy).sqrt();
            self.vx = dx / n * self.speed;
            self.vy = -(dy / n).abs() * self.speed;
            self.by = self.paddle_y - r;
            // where the next hit should land on the paddle: matches the way it will go
            self.aim = clamp(self.random() * 1.2 - 0.6, -0.6, 0.6);
        }
        if self.by > hf + r * 4.0 {
            // missed (it rarely does): serve again from the middle
            (self.bx, self.by) = (wf / 2.0, hf * 0.6);
            self.vx = self.speed * 0.4;
            self.vy = self.speed * 0.9;
            self.trail = [(self.bx, self.by); TRAIL];
        }
        // the bricks: the first one the ball overlaps breaks and turns it around
        if self.drop() >= 1.0 {
            'hit: for row in 0..ROWS {
                for col in 0..self.cols {
                    if !self.alive[row * self.cols + col] {
                        continue;
                    }
                    let (x, y) = self.brick_rect(row, col);
                    let (nx, ny) = (clamp(self.bx, x, x + self.brick_w), clamp(self.by, y, y + self.brick_h));
                    let (dx, dy) = (self.bx - nx, self.by - ny);
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    self.alive[row * self.cols + col] = false;
                    // which side: the smaller overlap
                    let over_x = if self.vx > 0.0 { self.bx + r - x } else { x + self.brick_w - (self.bx - r) };
                    let over_y = if self.vy > 0.0 { self.by + r - y } else { y + self.brick_h - (self.by - r) };
                    if over_x < over_y { self.vx = -self.vx } else { self.vy = -self.vy }
                    // the pieces: a burst in the brick's colour
                    let color = COLORS[row % COLORS.len()];
                    for _ in 0..10 {
                        let (px, py) = (x + self.random() * self.brick_w, y + self.random() * self.brick_h);
                        let (vx, vy) = ((self.random() - 0.5) * self.speed * 0.9, (self.random() - 0.7) * self.speed * 0.9);
                        self.pieces.push(Piece { x: px, y: py, vx, vy, life: 40, color });
                    }
                    break 'hit;
                }
            }
            if self.alive.iter().all(|a| !a) {
                self.new_wall();
            }
        }
        let g = hf / 4000.0;
        for p in &mut self.pieces {
            p.x += p.vx;
            p.y += p.vy;
            p.vy += g * 6.0;
            p.life = p.life.saturating_sub(1);
        }
        self.pieces.retain(|p| p.life > 0 && p.y < hf);

        // draw: transparent, then the bricks, the pieces, the paddle, the trail and the ball
        self.fb.fill(0);
        let (bw, bh) = (self.brick_w, self.brick_h);
        let edge = (bh / 6.0).max(1.0) as i32 as f32;
        for row in 0..ROWS {
            let c = COLORS[row % COLORS.len()];
            for col in 0..self.cols {
                if self.alive[row * self.cols + col] {
                    let (x, y) = self.brick_rect(row, col);
                    self.rect(x, y, bw, bh, c, 1.0);
                    self.rect(x, y, bw, edge, Self::lighter(c, 0.35), 1.0);
                    self.rect(x, y + bh - edge, bw, edge, Self::darker(c, 0.2), 1.0);
                }
            }
        }
        let pieces: Vec<(f32, f32, [u8; 3], f32)> = self.pieces.iter().map(|p| (p.x, p.y, p.color, p.life as f32 / 40.0)).collect();
        let size = (bh / 3.0).max(1.0);
        for (x, y, c, a) in pieces {
            self.rect(x, y, size, size, c, a);
        }
        let (px, py, pw, ph) = (self.paddle_x, self.paddle_y, self.paddle_w, self.paddle_h);
        let fg = self.fg;
        self.rect(px, py, pw, ph, fg, 1.0);
        self.rect(px, py, ph, ph, COLORS[0], 1.0);
        self.rect(px + pw - ph, py, ph, ph, COLORS[4], 1.0);
        let trail = self.trail;
        for (i, (x, y)) in trail.into_iter().enumerate().rev() {
            self.disc(x, y, r * (1.0 - i as f32 / TRAIL as f32 * 0.5), fg, 0.35 - i as f32 * 0.04);
        }
        let (bx, by) = (self.bx, self.by);
        self.disc(bx, by, r, fg, 1.0);
        gasm::present(&self.fb, self.w as u32, self.h as u32, self.w as u32 * 4);
    }
}

gasm::game!(Bricks);
