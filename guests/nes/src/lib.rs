//! NES emulator for gasm, built on [tetanes-core](https://crates.io/crates/tetanes-core)
//! (MIT/Apache-2.0).
//!
//! Assets: `rom` (iNES file). Parameters: `filter=ntsc` for the composite
//! video filter (default: sharp pixels), `quit_at=N` to log hashes and exit
//! after N frames.
//!
//! Configured for determinism: power-on RAM is zeroed (not random) and the core
//! does no filesystem access (battery saves are a TODO for `gasm:storage`).

use gasm::{Buttons, log};
use tetanes_core::control_deck::{Config, ControlDeck};
use tetanes_core::input::{JoypadBtn, Player};
use tetanes_core::memory::RamState;
use tetanes_core::video::VideoFilter;

const SAMPLE_RATE: u32 = 48_000;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const MAP: [(u32, JoypadBtn); 8] = [
    (Buttons::A, JoypadBtn::A),
    (Buttons::B, JoypadBtn::B),
    (Buttons::SELECT, JoypadBtn::Select),
    (Buttons::START, JoypadBtn::Start),
    (Buttons::UP, JoypadBtn::Up),
    (Buttons::DOWN, JoypadBtn::Down),
    (Buttons::LEFT, JoypadBtn::Left),
    (Buttons::RIGHT, JoypadBtn::Right),
];

pub struct Nes {
    deck: ControlDeck,
    frames: u32,
    quit_at: Option<u32>,
}

impl gasm::Game for Nes {
    fn init() -> Result<Self, String> {
        let rom = gasm::asset("rom").ok_or("no asset named \"rom\" (pass one to the runner)")?;
        let filter = match gasm::param("filter").as_deref() {
            Some("ntsc") => VideoFilter::Ntsc,
            _ => VideoFilter::Pixellate,
        };
        let mut cfg = Config::default();
        cfg.filter = filter;
        cfg.ram_state = RamState::AllZeros;
        cfg.sram_dir = None;
        let mut deck = ControlDeck::with_config(cfg);
        deck.set_sample_rate(SAMPLE_RATE as f32);
        let loaded = deck.load_rom("rom", &mut rom.as_slice()).map_err(|e| format!("cannot load rom: {e}"))?;
        log!("[nes] tetanes-core: loaded {} byte rom ({:?})", rom.len(), loaded);

        // NTSC NES: 60.0988 Hz (PAL carts would be 50.007 Hz)
        let region = deck.cart_region();
        let fps = if matches!(region, Some(r) if format!("{r:?}").contains("Pal")) { 50.007 } else { 60.0988 };
        gasm::set_frame_rate(fps);
        gasm::audio::config(SAMPLE_RATE, 1);
        let quit_at = gasm::param("quit_at").and_then(|v| v.parse().ok());
        Ok(Nes { deck, frames: 0, quit_at })
    }

    fn frame(&mut self) {
        for (player, slot) in [(0, Player::One), (1, Player::Two)] {
            let pad = gasm::pad(player);
            let joypad = self.deck.joypad_mut(slot);
            for (bit, btn) in MAP {
                joypad.set_button(btn, pad.held(bit));
            }
        }
        if let Err(e) = self.deck.clock_frame() {
            log!("[nes] emulation error: {e}");
        }
        gasm::present(self.deck.frame_buffer(), WIDTH, HEIGHT, WIDTH * 4);
        gasm::audio::push(self.deck.audio_samples());

        self.frames += 1;
        if self.quit_at == Some(self.frames) {
            gasm::exit(0);
        }
    }
}

gasm::game!(Nes);
