//! Deterministic simulation. Only IEEE-exact float ops (+ - * / sqrt), no
//! transcendental functions, clocks or randomness: the same inputs produce a
//! bit-identical `Sim` on every runner, which is what lockstep relies on.
//! (Rust never fuses multiply-adds, so this holds for native builds too.)

pub const ARENA_R: f32 = 8.0;
pub const BALL_R: f32 = 1.0;
pub const WIN_SCORE: i32 = 5;

pub const COUNTDOWN_FRAMES: i32 = 120;
pub const ROUND_OVER_FRAMES: i32 = 120;
pub const MATCH_OVER_FRAMES: i32 = 300;

const DT: f32 = 1.0 / 60.0;
const ACCEL: f32 = 38.0; // units/s^2 from the stick
const DAMPING: f32 = 0.92; // per-frame velocity retention on the platform
const DASH_SPEED: f32 = 14.0;
const DASH_FRAMES: i32 = 8;
const DASH_COOLDOWN: i32 = 50;
const GRAVITY: f32 = 30.0;
const RESTITUTION: f32 = 1.15; // > 1: collisions are a bit bouncy, which is fun
const DASH_KNOCK: f32 = 1.6; // extra push while the other ball is dashing
const FALL_OUT_Y: f32 = -12.0;

/// World-space input bits (independent of camera, see lib.rs).
pub const IN_UP: u16 = 1;
pub const IN_DOWN: u16 = 2;
pub const IN_LEFT: u16 = 4;
pub const IN_RIGHT: u16 = 8;
pub const IN_DASH: u16 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Phase {
    Countdown,
    Fight,
    RoundOver,
    MatchOver,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Player {
    pub x: f32,
    pub y: f32, // 0 on the platform, < 0 when falling
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub dash_cd: i32, // frames until the next dash
    pub dash_t: i32,  // frames left in the current dash
    pub falling: i32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Sim {
    pub frame: u32,
    pub round: u32,
    pub phase: Phase,
    pub phase_t: i32,
    pub round_winner: i32, // -1 draw
    pub score: [i32; 2],
    pub p: [Player; 2],
}

impl Sim {
    pub fn new() -> Sim {
        let mut s = Sim {
            frame: 0,
            round: 0,
            phase: Phase::Countdown,
            phase_t: 0,
            round_winner: -1,
            score: [0; 2],
            p: [Player::default(); 2],
        };
        s.place();
        s
    }

    fn place(&mut self) {
        self.p = [Player::default(); 2];
        self.p[0].x = -4.0;
        self.p[1].x = 4.0;
    }

    pub fn step(&mut self, inputs: [u16; 2]) {
        self.frame += 1;
        self.phase_t += 1;
        match self.phase {
            Phase::Countdown => {
                if self.phase_t >= COUNTDOWN_FRAMES {
                    self.phase = Phase::Fight;
                    self.phase_t = 0;
                }
                return;
            }
            Phase::Fight => {
                control(&mut self.p[0], inputs[0]);
                control(&mut self.p[1], inputs[1]);
            }
            Phase::RoundOver | Phase::MatchOver => {} // no control, physics keeps running
        }
        integrate(&mut self.p[0]);
        integrate(&mut self.p[1]);
        let [a, b] = &mut self.p;
        collide(a, b);

        match self.phase {
            Phase::Fight => {
                let (out0, out1) = (self.p[0].y < -1.0, self.p[1].y < -1.0);
                if out0 || out1 {
                    self.round_winner = if out0 && out1 { -1 } else if out0 { 1 } else { 0 };
                    if self.round_winner >= 0 {
                        self.score[self.round_winner as usize] += 1;
                    }
                    let over = self.score.iter().any(|&s| s >= WIN_SCORE);
                    self.phase = if over { Phase::MatchOver } else { Phase::RoundOver };
                    self.phase_t = 0;
                }
            }
            Phase::RoundOver if self.phase_t >= ROUND_OVER_FRAMES => {
                self.round += 1;
                self.place();
                self.phase = Phase::Countdown;
                self.phase_t = 0;
            }
            Phase::MatchOver if self.phase_t >= MATCH_OVER_FRAMES => {
                let frame = self.frame;
                *self = Sim::new();
                self.frame = frame;
            }
            _ => {}
        }
        for p in &mut self.p {
            if p.y < FALL_OUT_Y {
                p.y = FALL_OUT_Y;
                (p.vx, p.vy, p.vz) = (0.0, 0.0, 0.0);
            }
        }
    }

    /// FNV-1a over the raw state (repr(C), no padding: all fields are 4 bytes).
    pub fn hash(&self) -> u32 {
        let bytes =
            unsafe { std::slice::from_raw_parts(self as *const Sim as *const u8, std::mem::size_of::<Sim>()) };
        bytes.iter().fold(0x811c_9dc5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
    }
}

impl Default for Sim {
    fn default() -> Self {
        Sim::new()
    }
}

fn input_dir(input: u16) -> (f32, f32) {
    let mut x = 0.0;
    let mut z = 0.0;
    if input & IN_UP != 0 { z -= 1.0; }
    if input & IN_DOWN != 0 { z += 1.0; }
    if input & IN_LEFT != 0 { x -= 1.0; }
    if input & IN_RIGHT != 0 { x += 1.0; }
    if x != 0.0 && z != 0.0 {
        x *= 0.707_106_78;
        z *= 0.707_106_78;
    }
    (x, z)
}

fn control(p: &mut Player, input: u16) {
    if p.falling != 0 {
        return;
    }
    let (mut dx, mut dz) = input_dir(input);
    p.vx += dx * ACCEL * DT;
    p.vz += dz * ACCEL * DT;
    if p.dash_cd > 0 { p.dash_cd -= 1; }
    if p.dash_t > 0 { p.dash_t -= 1; }
    if input & IN_DASH != 0 && p.dash_cd == 0 {
        if dx == 0.0 && dz == 0.0 {
            // no stick: dash along the current velocity
            let v = (p.vx * p.vx + p.vz * p.vz).sqrt();
            if v > 0.001 {
                (dx, dz) = (p.vx / v, p.vz / v);
            }
        }
        if dx != 0.0 || dz != 0.0 {
            p.vx = dx * DASH_SPEED;
            p.vz = dz * DASH_SPEED;
            p.dash_t = DASH_FRAMES;
            p.dash_cd = DASH_COOLDOWN;
        }
    }
}

fn integrate(p: &mut Player) {
    if p.falling == 0 {
        p.vx *= DAMPING;
        p.vz *= DAMPING;
        if p.x * p.x + p.z * p.z > ARENA_R * ARENA_R {
            p.falling = 1;
        }
    } else {
        p.vy -= GRAVITY * DT;
        p.vx *= 0.99;
        p.vz *= 0.99;
    }
    p.x += p.vx * DT;
    p.y += p.vy * DT;
    p.z += p.vz * DT;
}

fn collide(a: &mut Player, b: &mut Player) {
    let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let d2 = dx * dx + dy * dy + dz * dz;
    let min = 2.0 * BALL_R;
    if d2 >= min * min || d2 < 1e-8 {
        return;
    }
    let d = d2.sqrt();
    let (nx, ny, nz) = (dx / d, dy / d, dz / d);
    // separate
    let push = (min - d) * 0.5;
    a.x -= nx * push;
    a.y -= ny * push;
    a.z -= nz * push;
    b.x += nx * push;
    b.y += ny * push;
    b.z += nz * push;
    if a.falling == 0 { a.y = 0.0; }
    if b.falling == 0 { b.y = 0.0; }
    // impulse along the normal (equal masses)
    let vn = (b.vx - a.vx) * nx + (b.vz - a.vz) * nz;
    if vn >= 0.0 {
        return;
    }
    let j = -RESTITUTION * vn * 0.5;
    let ja = j * if b.dash_t > 0 { DASH_KNOCK } else { 1.0 };
    let jb = j * if a.dash_t > 0 { DASH_KNOCK } else { 1.0 };
    a.vx -= nx * ja;
    a.vz -= nz * ja;
    b.vx += nx * jb;
    b.vz += nz * jb;
}

/// Simple deterministic opponent for offline play and tests.
pub fn bot_input(s: &Sim, me: usize) -> u16 {
    let (p, o) = (&s.p[me], &s.p[1 - me]);
    let (mut tx, mut tz) = (o.x - p.x, o.z - p.z);
    let edge = ARENA_R - 2.2;
    if p.x * p.x + p.z * p.z > edge * edge {
        (tx, tz) = (-p.x, -p.z); // back off the edge
    }
    let mut input = 0;
    if tx < -0.3 { input |= IN_LEFT; }
    if tx > 0.3 { input |= IN_RIGHT; }
    if tz < -0.3 { input |= IN_UP; }
    if tz > 0.3 { input |= IN_DOWN; }
    let d2 = (o.x - p.x) * (o.x - p.x) + (o.z - p.z) * (o.z - p.z);
    if d2 < 3.5 * 3.5 && (s.frame / 7) % 3 == 0 {
        input |= IN_DASH;
    }
    input
}
