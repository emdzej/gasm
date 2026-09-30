//! sumo: two-player 3D arena for gasm (gasm:gfx + gasm:net).
//!
//! Push the other ball off the platform. Stick/arrows move, A or B (X/Z keys) dash.
//!
//! Parameters (`--param k=v` natively, `?k=v` in the browser):
//! - `relay=ws://host:port`: play online through gasm-relay (lockstep)
//! - `room=name`: relay room (default `sumo`)
//! - `mode=bot|local2`: offline vs. a bot (default) or two local pads
//! - `quit_at=N`: log the state hash at sim frame N and exit (tests)
//!
//! Your win/loss record is kept in `gasm:storage` (key `record`).
//!
//! Networking is deterministic lockstep. Peers exchange only inputs, scheduled
//! `INPUT_DELAY` frames ahead, and each runs the same [`Sim`]. A state hash is
//! exchanged every `HASH_EVERY` frames to detect desyncs.

mod math;
mod render;
pub mod sim;

use gasm::net::{Conn, Recv, State};
use gasm::{Buttons, log};

use render::Renderer;
use sim::{Sim, IN_DASH, IN_DOWN, IN_LEFT, IN_RIGHT, IN_UP};

const INPUT_DELAY: u32 = 4;
const RING: usize = 256;
const HASH_EVERY: u32 = 120;

// gasm-relay protocol (runners/native/src/bin/gasm-relay.rs)
const R_WELCOME: u8 = 1;
const R_JOIN: u8 = 2;
const R_LEAVE: u8 = 3;
const R_FULL: u8 = 4;
const R_DATA: u8 = 0x10;
// game messages inside R_DATA
const M_INPUT: u8 = 1;
const M_HASH: u8 = 2;

/// Map pad buttons to world-space input. Player 2's camera looks from the
/// other side, so their directions are mirrored to feel natural on screen.
fn read_pad(player: u32, mirrored: bool) -> u16 {
    let pad = gasm::pad(player);
    let (up, down, left, right) =
        if mirrored { (IN_DOWN, IN_UP, IN_RIGHT, IN_LEFT) } else { (IN_UP, IN_DOWN, IN_LEFT, IN_RIGHT) };
    let mut input = 0;
    if pad.held(Buttons::UP) { input |= up; }
    if pad.held(Buttons::DOWN) { input |= down; }
    if pad.held(Buttons::LEFT) { input |= left; }
    if pad.held(Buttons::RIGHT) { input |= right; }
    if pad.held(Buttons::A | Buttons::B) { input |= IN_DASH; }
    input
}

/// Lockstep session state for one relay connection.
struct Online {
    conn: Conn,
    local: usize, // our player index (relay join order)
    peers: u32,
    running: bool,
    /// The opponent left; finish the frames we have inputs for, then stop.
    leaving: bool,
    /// inputs[player][frame % RING] = (frame + 1, bits); frame + 1 so 0 means empty
    inputs: [[(u32, u16); RING]; 2],
    hashes: [(u32, u32); RING], // (frame + 1, local hash) per HASH_EVERY slot
    stalls: u32,
    last_wait_log: u32,
}

impl Online {
    fn set_input(&mut self, player: usize, frame: u32, bits: u16) {
        self.inputs[player][frame as usize % RING] = (frame + 1, bits);
    }
    fn input(&self, player: usize, frame: u32) -> Option<u16> {
        let (f, bits) = self.inputs[player][frame as usize % RING];
        (f == frame + 1).then_some(bits)
    }
    fn send(&self, msg: &[u8]) {
        let mut buf = Vec::with_capacity(msg.len() + 1);
        buf.push(R_DATA);
        buf.extend_from_slice(msg);
        self.conn.send(&buf);
    }
}

/// Wins/losses, persisted as two little-endian u32s (separately vs bot and online).
#[derive(Default, Clone, Copy)]
struct Record {
    wins: u32,
    losses: u32,
}

impl Record {
    fn load(key: &str) -> Record {
        match gasm::storage::get(key) {
            Some(b) if b.len() >= 8 => Record {
                wins: u32::from_le_bytes(b[0..4].try_into().unwrap()),
                losses: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            },
            _ => Record::default(),
        }
    }
    fn save(&self, key: &str) {
        let mut b = self.wins.to_le_bytes().to_vec();
        b.extend_from_slice(&self.losses.to_le_bytes());
        gasm::storage::set(key, &b);
    }
}

pub struct Sumo {
    record_key: &'static str,
    record: Record,
    recorded: bool, // current match already counted
    sim: Sim,
    renderer: Renderer,
    ticks: u32, // runner frames, for animation
    quit_at: Option<u32>,
    local2: bool,
    online: Option<Online>,
}

impl Sumo {
    fn check_quit(&self) {
        if self.quit_at.is_some_and(|q| self.sim.frame >= q) {
            log!(
                "[sumo] frame {} state={:08x} score={}:{}",
                self.sim.frame,
                self.sim.hash(),
                self.sim.score[0],
                self.sim.score[1]
            );
            gasm::exit(0);
        }
    }

    fn start_session(&mut self) {
        let net = self.online.as_mut().unwrap();
        self.sim = Sim::new();
        net.inputs = [[(0, 0); RING]; 2];
        net.hashes = [(0, 0); RING];
        for f in 0..INPUT_DELAY {
            net.set_input(0, f, 0);
            net.set_input(1, f, 0);
        }
        net.running = true;
        net.leaving = false;
        net.stalls = 0;
        log!("[sumo] match start: you are player {} ({})", net.local + 1, ["red", "blue"][net.local]);
    }

    fn poll_net(&mut self) {
        loop {
            let net = self.online.as_mut().unwrap();
            let msg = match net.conn.recv() {
                Recv::Message(m) if !m.is_empty() => m,
                Recv::Message(_) => continue,
                Recv::Empty => break,
                Recv::Closed => {
                    if net.running {
                        log!("[sumo] disconnected from relay");
                        net.running = false;
                    }
                    break;
                }
            };
            match msg[0] {
                R_WELCOME if msg.len() >= 3 => {
                    net.local = msg[1] as usize & 1;
                    net.peers = msg[2] as u32;
                    log!("[sumo] joined room as peer {} ({} present)", net.local, net.peers);
                    if net.peers == 2 {
                        self.start_session();
                    }
                }
                R_JOIN => {
                    net.peers += 1;
                    log!("[sumo] opponent joined");
                    if net.peers == 2 {
                        self.start_session();
                    }
                }
                R_LEAVE => {
                    net.peers = net.peers.saturating_sub(1);
                    net.leaving = net.running; // handled after stepping (see frame())
                    log!("[sumo] opponent left");
                }
                R_FULL => log!("[sumo] room is full"),
                R_DATA if net.running && msg.len() > 2 => {
                    let remote = 1 - net.local;
                    let m = &msg[2..]; // [R_DATA][from][payload]
                    let u32_at = |i: usize| u32::from_le_bytes(m[i..i + 4].try_into().unwrap());
                    if m[0] == M_INPUT && m.len() >= 7 {
                        net.set_input(remote, u32_at(1), u16::from_le_bytes([m[5], m[6]]));
                    } else if m[0] == M_HASH && m.len() >= 9 {
                        let (f, h) = (u32_at(1), u32_at(5));
                        let (lf, lh) = net.hashes[(f / HASH_EVERY) as usize % RING];
                        if lf == f + 1 && lh != h {
                            log!("[sumo] DESYNC at frame {f}: local {lh:08x} remote {h:08x}");
                        } else if lf == f + 1 && f % (HASH_EVERY * 10) == 0 {
                            log!("[sumo] in sync at frame {f} ({h:08x})");
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Advance as far as confirmed inputs allow (max 2 steps per runner frame,
    /// so a lagging peer catches up smoothly). Returns whether it advanced.
    fn step_online(&mut self) -> bool {
        let local = self.online.as_ref().unwrap().local;
        let pad = read_pad(0, local == 1);
        let mut stepped = false;
        for _ in 0..2 {
            let net = self.online.as_mut().unwrap();
            let f = self.sim.frame;
            let (Some(i0), Some(i1)) = (net.input(0, f), net.input(1, f)) else { break };
            // schedule our input INPUT_DELAY frames ahead and send it
            let target = f + INPUT_DELAY;
            if net.input(local, target).is_none() {
                net.set_input(local, target, pad);
                let mut msg = vec![M_INPUT];
                msg.extend_from_slice(&target.to_le_bytes());
                msg.extend_from_slice(&pad.to_le_bytes());
                net.send(&msg);
            }
            self.sim.step([i0, i1]);
            stepped = true;
            if self.sim.frame % HASH_EVERY == 0 {
                let h = self.sim.hash();
                net.hashes[(self.sim.frame / HASH_EVERY) as usize % RING] = (self.sim.frame + 1, h);
                let mut msg = vec![M_HASH];
                msg.extend_from_slice(&self.sim.frame.to_le_bytes());
                msg.extend_from_slice(&h.to_le_bytes());
                net.send(&msg);
            }
            self.check_quit();
        }
        let net = self.online.as_mut().unwrap();
        if !stepped && !net.leaving {
            net.stalls += 1;
            if net.stalls % 60 == 0 {
                log!("[sumo] waiting for opponent input (frame {})", self.sim.frame);
            }
        }
        stepped
    }
}

impl Sumo {
    /// Count a finished match once for the local player (not in two-local-players mode).
    fn count_result(&mut self, local: usize) {
        if self.sim.phase != sim::Phase::MatchOver {
            self.recorded = false;
            return;
        }
        if self.recorded || self.local2 {
            return;
        }
        self.recorded = true;
        if self.sim.score[local] >= sim::WIN_SCORE {
            self.record.wins += 1;
            log!("[sumo] you win! record: {} wins, {} losses", self.record.wins, self.record.losses);
        } else {
            self.record.losses += 1;
            log!("[sumo] you lose. record: {} wins, {} losses", self.record.wins, self.record.losses);
        }
        self.record.save(self.record_key);
    }
}

impl gasm::Game for Sumo {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let quit_at = gasm::param("quit_at").and_then(|v| v.parse().ok());
        let local2 = gasm::param("mode").as_deref() == Some("local2");
        let online = gasm::param("relay").and_then(|relay| {
            let room = gasm::param("room").unwrap_or_else(|| "sumo".into());
            let url = format!("{}/{room}", relay.trim_end_matches('/'));
            match Conn::open(&url) {
                Some(conn) => {
                    log!("[sumo] connecting to {url}");
                    Some(Online {
                        conn,
                        local: 0,
                        peers: 0,
                        running: false,
                        leaving: false,
                        inputs: [[(0, 0); RING]; 2],
                        hashes: [(0, 0); RING],
                        stalls: 0,
                        last_wait_log: 0,
                    })
                }
                None => {
                    log!("[sumo] cannot open {url} (network not allowed?); playing offline");
                    None
                }
            }
        });
        if online.is_none() {
            log!("[sumo] offline mode: {}", if local2 { "two local players" } else { "vs. bot" });
        }
        let record_key = if online.is_some() { "record-online" } else { "record-bot" };
        let record = Record::load(record_key);
        if !local2 {
            log!("[sumo] your record ({}): {} wins, {} losses", &record_key[7..], record.wins, record.losses);
        }
        Ok(Sumo {
            record_key,
            record,
            recorded: false,
            sim: Sim::new(),
            renderer: Renderer::new(),
            ticks: 0,
            quit_at,
            local2,
            online,
        })
    }

    fn frame(&mut self) {
        self.ticks += 1;
        let (local, waiting) = if self.online.is_some() {
            self.poll_net();
            let net = self.online.as_mut().unwrap();
            if net.running {
                let advanced = self.step_online();
                let net = self.online.as_mut().unwrap();
                if net.leaving && !advanced {
                    net.running = false;
                    net.leaving = false;
                    log!("[sumo] match ended at frame {}; waiting for an opponent", self.sim.frame);
                    self.sim = Sim::new();
                }
            } else if self.ticks - net.last_wait_log > 300 && net.conn.state() == State::Open {
                net.last_wait_log = self.ticks;
                log!("[sumo] waiting for an opponent...");
            }
            let net = self.online.as_ref().unwrap();
            (net.local, !net.running)
        } else {
            let in0 = read_pad(0, false);
            let in1 = if self.local2 { read_pad(1, false) } else { sim::bot_input(&self.sim, 1) };
            self.sim.step([in0, in1]);
            self.check_quit();
            (0, false)
        };
        self.count_result(local);
        self.renderer.frame(&self.sim, local, waiting, self.ticks as f32 / 60.0);
    }
}

gasm::game!(Sumo);
