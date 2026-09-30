//! NES emulator for gasm, built on [tetanes-core](https://crates.io/crates/tetanes-core)
//! (MIT/Apache-2.0).
//!
//! Assets: `rom` (iNES file). Parameters: `filter=ntsc` for the composite
//! video filter (default: sharp pixels), `quit_at=N` to log hashes and exit
//! after N frames.
//!
//! Configured for determinism: power-on RAM is zeroed (not random) and the core
//! does no filesystem access. Battery-backed carts keep their save RAM in
//! `gasm:storage` under `sram-<rom hash>` (raw PRG-RAM, like a `.srm` file),
//! written when it changes (checked every ~5 s) and when the player quits.

use gasm::{Buttons, log};
use tetanes_core::control_deck::{Config, ControlDeck};
use tetanes_core::input::{JoypadBtn, Player};
use tetanes_core::memory::RamState;
use tetanes_core::video::VideoFilter;

const SAMPLE_RATE: u32 = 48_000;
const SAVE_CHECK_FRAMES: u32 = 300;
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
    /// storage key for battery saves (None: cart has no battery)
    save_key: Option<String>,
    saved_hash: u32,
}

fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

impl Nes {
    /// Persist save RAM if it changed since the last write.
    fn flush_save(&mut self) {
        let Some(key) = &self.save_key else { return };
        let sram = self.deck.sram();
        let h = fnv(sram);
        if h != self.saved_hash && !sram.is_empty() {
            if gasm::storage::set(key, sram) {
                self.saved_hash = h;
            } else {
                log!("[nes] could not write save {key}");
            }
        }
    }
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

        let save_key = (deck.cart_battery_backed() == Some(true)).then(|| format!("sram-{:08x}", fnv(&rom)));
        let mut saved_hash = 0;
        if let Some(key) = &save_key {
            match gasm::storage::get(key) {
                Some(save) => {
                    deck.set_sram(&save);
                    saved_hash = fnv(deck.sram());
                    log!("[nes] restored battery save {key} ({} bytes)", save.len());
                }
                None => saved_hash = fnv(deck.sram()),
            }
        }
        Ok(Nes { deck, frames: 0, quit_at, save_key, saved_hash })
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
        if self.frames % SAVE_CHECK_FRAMES == 0 {
            self.flush_save();
        }
        if self.quit_at == Some(self.frames) {
            self.flush_save();
            gasm::exit(0);
        }
    }

    fn exit(&mut self) {
        self.flush_save();
    }
}

gasm::game!(Nes);
