//! Native build of the NES guest, driven through the same ABI entry points a
//! runner calls. Prints the same hash lines as `gasm-run --headless`, so the
//! wasm build can be compared against native code (`make parity`).
//!
//!   parity <rom> <frames> [--no-hash]

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: parity <rom> <frames> [--no-hash]");
        std::process::exit(2);
    }
    let frames: u64 = args[2].parse().expect("frames must be a number");
    gasm::native::set_asset("rom", std::fs::read(&args[1]).expect("cannot read rom"));
    gasm::native::set_hashing(!args.iter().any(|a| a == "--no-hash"));

    assert_eq!(nes::gasm_abi_version(), 0);
    assert_eq!(nes::gasm_init(), 0, "gasm_init failed");
    let t0 = Instant::now();
    for _ in 0..frames {
        nes::gasm_frame();
        gasm::native::end_frame();
    }
    let secs = t0.elapsed().as_secs_f64();
    let s = gasm::native::stats();
    println!("frames={frames} presented={} size={}x{}", s.presented, s.width, s.height);
    println!("video_fnv32={:08x} audio_fnv32={:08x} audio_frames={}", s.video_hash, s.audio_hash, s.audio_frames);
    eprintln!("[parity] {:.1} frames/s ({:.1}x realtime)", frames as f64 / secs, frames as f64 / secs / s.frame_rate);
}
