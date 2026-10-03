use std::collections::HashMap;
use gasm_host::{assets::Assets, gfx::Gfx, host::{Game, Host, LoadOptions}, net::Net, storage::Storage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = Host::new(Assets::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
    let mut game = Game::load(&std::fs::read("game.wasm")?, host, LoadOptions::default())?;
    for _ in 0..600 {
        game.frame()?;                    // call at game.with_host(|h| h.frame_rate) Hz
    }
    // with_host works for every guest (a gasm_run guest owns its host while it runs)
    let (w, h, hash) = game.with_host(|h| (h.width, h.height, h.video_hash.0));
    println!("{w}x{h}, video hash {hash:08x}");
    Ok(())
}

