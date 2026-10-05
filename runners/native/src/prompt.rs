//! The window runner's consent question (consent.rs) as an image: drawn instead of the
//! game's frame while the game waits for the answer (keys 1-4, Esc = not now).

use font8x8::legacy::BASIC_LEGACY;

pub const W: usize = 640;
pub const H: usize = 360;

const BG: [u8; 3] = [0x14, 0x17, 0x1e];
const PANEL: [u8; 3] = [0x1f, 0x24, 0x2e];
const TEXT: [u8; 3] = [0xe8, 0xea, 0xee];
const DIM: [u8; 3] = [0x8a, 0x91, 0x9e];
const ACCENT: [u8; 3] = [0x4e, 0xcd, 0xc4];

fn fill(img: &mut [u8], x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
    for yy in y..(y + h).min(H) {
        for xx in x..(x + w).min(W) {
            img[(yy * W + xx) * 4..][..4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

/// ASCII text at (x, y), `scale` pixels per glyph pixel; other characters show as '?'.
/// Returns the x after it.
fn text(img: &mut [u8], x: usize, y: usize, s: &str, scale: usize, c: [u8; 3]) -> usize {
    let mut cx = x;
    for ch in s.chars() {
        let glyph = BASIC_LEGACY[if ch.is_ascii() && !ch.is_ascii_control() { ch as usize } else { b'?' as usize }];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits >> col & 1 == 1 {
                    fill(img, cx + col * scale, y + row * scale, scale, scale, c);
                }
            }
        }
        cx += 8 * scale;
    }
    cx
}

/// Cut to `max` characters, with "..." at the end.
fn fit(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_owned() } else { s.chars().take(max - 3).collect::<String>() + "..." }
}

/// The question about `subject` for `game` (RGBA8, W x H).
pub fn image(game: &str, subject: &str) -> Vec<u8> {
    let mut img = vec![0u8; W * H * 4];
    fill(&mut img, 0, 0, W, H, BG);
    fill(&mut img, 40, 30, W - 80, H - 60, PANEL);
    fill(&mut img, 40, 30, W - 80, 4, ACCENT);
    let (wants, what) = crate::consent::describe(subject);
    text(&mut img, 64, 52, &fit(game, 32), 2, TEXT);
    text(&mut img, 64, 76, wants, 2, DIM);
    text(&mut img, 64, 100, &fit(&what, 32), 2, ACCENT);
    let answers = [("1", "Allow this time"), ("2", "Always allow"), ("3", "Not now"), ("4", "Never (don't ask again)")];
    for (i, (key, what)) in answers.iter().enumerate() {
        let y = 140 + i * 30;
        fill(&mut img, 64, y - 4, 24, 24, ACCENT);
        text(&mut img, 68, y, key, 2, BG);
        text(&mut img, 100, y, what, 2, TEXT);
    }
    text(&mut img, 64, 272, "Press 1-4 (Esc: not now). The game waits.", 1, DIM);
    text(&mut img, 64, 286, "gasm-run --forget-consent <game> clears saved answers.", 1, DIM);
    img
}

#[cfg(test)]
mod tests {
    /// `GASM_PROMPT_PNG=/tmp/prompt.png cargo test --lib -- --ignored prompt` to look at it
    #[test]
    #[ignore]
    fn prompt_png() {
        let path = std::env::var("GASM_PROMPT_PNG").unwrap_or_else(|_| "prompt.png".into());
        let img = super::image("nowhereinparticular", "net:api.met.no");
        crate::headless::write_png(&path, super::W as u32, super::H as u32, &img).unwrap();
    }
}
