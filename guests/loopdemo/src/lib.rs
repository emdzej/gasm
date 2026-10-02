//! A game that keeps its own main loop ([`gasm::main_loop`]): two nested loops
//! (a title screen, then play) that call `wait_frame()` instead of returning.
//! The d-pad moves a square, A changes colour, START starts and, held, quits.

use gasm::{Buttons, main_loop::wait_frame};

const W: usize = 160;
const H: usize = 120;

fn present(fb: &[u8]) {
    gasm::present(fb, W as u32, H as u32, (W * 4) as u32);
}

fn fill(fb: &mut [u8], x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
    for j in y..(y + h).min(H) {
        for i in x..(x + w).min(W) {
            fb[(j * W + i) * 4..(j * W + i) * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

fn run() -> i32 {
    gasm::set_frame_rate(60.0);
    let mut fb = vec![0u8; W * H * 4];
    // title screen: its own loop, until START
    let mut t = 0u32;
    while !gasm::pad(0).held(Buttons::START) {
        fill(&mut fb, 0, 0, W, H, [10, 10, 30]);
        fill(&mut fb, 40 + (t / 4 % 60) as usize, 50, 20, 20, [250, 200, 60]);
        present(&fb);
        t += 1;
        wait_frame();
    }
    gasm::log!("loopdemo: title done after {t} frames");
    // wait until START is released, then play until it is held for a second
    while gasm::pad(0).held(Buttons::START) {
        wait_frame();
    }
    let (mut x, mut y, mut hue, mut held) = (70usize, 50usize, 0u8, 0u32);
    while held < 60 {
        let p = gasm::pad(0);
        held = if p.held(Buttons::START) { held + 1 } else { 0 };
        if p.held(Buttons::LEFT) { x = x.saturating_sub(1) }
        if p.held(Buttons::RIGHT) { x = (x + 1).min(W - 16) }
        if p.held(Buttons::UP) { y = y.saturating_sub(1) }
        if p.held(Buttons::DOWN) { y = (y + 1).min(H - 16) }
        if p.held(Buttons::A) { hue = hue.wrapping_add(5) }
        fill(&mut fb, 0, 0, W, H, [20, 30, 20]);
        fill(&mut fb, x, y, 16, 16, [hue, 255 - hue, 120]);
        present(&fb);
        wait_frame();
    }
    gasm::log!("loopdemo: done after {} frames", gasm::main_loop::frames());
    0
}

gasm::main_loop!(run);
