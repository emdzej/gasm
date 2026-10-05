//! Attract-mode Pong in the colours of gasm's icon: the website's background. Two
//! paddles keep a rally going; the right one follows the pointer's height while it
//! moves over the picture (and plays by itself again two seconds after it stops).
//!
//! Params `w`, `h`: the frame size (default 240×80), so the court has the page's shape;
//! paddles, ball and speed scale with the height and width.
//!
//! 2D frames (RGBA) with a transparent background, so the page's own colours
//! show through (light and dark); the page scales them up, pixel-crisp, and fades
//! them. Only `+ - * /` on floats: the same frames on every runner.

const TRAIL: usize = 6;

const NET: [u8; 3] = [0x3a, 0x42, 0x50];
const RED: [u8; 3] = [0xe8, 0x55, 0x4e];
const BLUE: [u8; 3] = [0x3b, 0x7f, 0xf5];
const WHITE: [u8; 3] = [0xf2, 0xf4, 0xf8];

struct Pong {
    w: usize,
    h: usize,
    paddle_w: f32,
    paddle_h: f32,
    ball: f32,
    left_x: f32,
    right_x: f32,
    speed: f32,
    bx: f32,
    by: f32,
    vx: f32,
    vy: f32,
    left: f32,
    right: f32,
    trail: [(f32, f32); TRAIL],
    /// frames since the pointer last moved over the picture (the right paddle is the player's until 120)
    idle: u32,
    last_pointer: Option<(f32, f32)>,
    frame: u32,
    fb: Vec<u8>,
}

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo { lo } else if v > hi { hi } else { v }
}

impl Pong {
    /// A computer paddle: towards where the ball is (while it comes its way), a little slow.
    fn follow(&self, paddle: f32, target: f32, coming: bool) -> f32 {
        let (ph, hf) = (self.paddle_h, self.h as f32);
        let want = if coming { target - ph / 2.0 } else { (hf - ph) / 2.0 };
        let max = hf / 72.0;
        let d = clamp(want - paddle, -max, max);
        clamp(paddle + d, 0.0, hf - ph)
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [u8; 3], a: u8) {
        let (x0, y0) = (x.max(0.0) as usize, y.max(0.0) as usize);
        let (x1, y1) = (((x + w) as usize).min(self.w), ((y + h) as usize).min(self.h));
        for yy in y0..y1 {
            for xx in x0..x1 {
                let i = (yy * self.w + xx) * 4;
                self.fb[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
            }
        }
    }
}

impl gasm::Game for Pong {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let num = |k: &str, d: usize, lo: usize, hi: usize| gasm::param(k).and_then(|v| v.parse().ok()).unwrap_or(d).clamp(lo, hi);
        let (w, h) = (num("w", 240, 64, 960), num("h", 80, 32, 360));
        let (wf, hf) = (w as f32, h as f32);
        let unit = (hf / 40.0).max(1.0) as i32 as f32;   // whole pixels: crisp when scaled up
        let (paddle_w, ball) = (unit * 1.5, unit * 1.5);
        let paddle_h = hf / 5.5;
        let speed = wf / 140.0;   // a crossing takes about two seconds at any width
        let (bx, by) = (wf / 2.0, hf / 3.0);
        Ok(Pong {
            w, h, paddle_w, paddle_h, ball,
            left_x: unit * 4.0, right_x: wf - unit * 4.0 - paddle_w, speed,
            bx, by, vx: speed, vy: speed * 0.35,
            left: (hf - paddle_h) / 2.0, right: (hf - paddle_h) / 2.0,
            trail: [(bx, by); TRAIL],
            idle: 1000, last_pointer: None, frame: 0,
            fb: vec![0; w * h * 4],
        })
    }

    fn frame(&mut self) {
        self.frame += 1;
        // the pointer: the right paddle is the player's while it moves over the picture
        self.idle = self.idle.saturating_add(1);
        if let Some(p) = gasm::input::pointer().filter(|p| p.inside()) {
            let at = (p.frame_x, p.frame_y);
            if self.last_pointer.is_some_and(|l| l != at) {
                self.idle = 0;
            }
            self.last_pointer = Some(at);
            if self.idle < 120 {
                self.right = clamp(at.1 - self.paddle_h / 2.0, 0.0, self.h as f32 - self.paddle_h);
            }
        }
        let (pw, ph, ball, hf, wf) = (self.paddle_w, self.paddle_h, self.ball, self.h as f32, self.w as f32);
        let (lx, rx) = (self.left_x, self.right_x);
        let ball_mid = self.by + ball / 2.0;
        self.left = self.follow(self.left, ball_mid, self.vx < 0.0);
        if self.idle >= 120 {
            self.right = self.follow(self.right, ball_mid, self.vx > 0.0);
        }

        // the ball: walls, paddles (the angle depends on where it hits), and a new
        // serve from the middle if a paddle misses
        self.trail.rotate_right(1);
        self.trail[0] = (self.bx, self.by);
        self.bx += self.vx;
        self.by += self.vy;
        if self.by < 0.0 || self.by > hf - ball {
            self.vy = -self.vy;
            self.by = clamp(self.by, 0.0, hf - ball);
        }
        let hit = |paddle: f32, by: f32| by + ball > paddle && by < paddle + ph;
        if self.vx < 0.0 && self.bx <= lx + pw && self.bx > lx - ball && hit(self.left, self.by) {
            self.vx = -self.vx;
            self.vy = (self.by + ball / 2.0 - (self.left + ph / 2.0)) / (ph / 2.0) * self.speed * 0.7;
            self.bx = lx + pw;
        }
        if self.vx > 0.0 && self.bx + ball >= rx && self.bx < rx + pw && hit(self.right, self.by) {
            self.vx = -self.vx;
            self.vy = (self.by + ball / 2.0 - (self.right + ph / 2.0)) / (ph / 2.0) * self.speed * 0.7;
            self.bx = rx - ball;
        }
        if self.bx < -ball * 4.0 || self.bx > wf + ball * 4.0 {
            let serve_right = self.bx < 0.0;
            (self.bx, self.by) = (wf / 2.0, hf / 2.0);
            self.vx = if serve_right { self.speed } else { -self.speed };
            self.vy = self.speed * if self.frame % 2 == 0 { 0.35 } else { -0.35 };
            self.trail = [(self.bx, self.by); TRAIL];
        }

        // draw: transparent, then the net, the paddles, the trail and the ball
        self.fb.fill(0);
        let unit = pw / 1.5;
        let mut y = unit * 2.0;
        while y < hf {
            self.rect((wf / 2.0 - unit / 2.0) as i32 as f32, y, unit, unit * 4.0, NET, 255);
            y += unit * 8.0;
        }
        let (l, r) = (self.left, self.right);
        self.rect(lx, l, pw, ph, RED, 255);
        self.rect(rx, r, pw, ph, BLUE, 255);
        for (i, (x, y)) in self.trail.into_iter().enumerate().rev() {
            let a = (160 - i * 24) as u8;
            self.rect(x, y, ball, ball, WHITE, a);
        }
        let (bx, by) = (self.bx, self.by);
        self.rect(bx, by, ball, ball, WHITE, 255);
        gasm::present(&self.fb, self.w as u32, self.h as u32, self.w as u32 * 4);
    }
}

gasm::game!(Pong);
