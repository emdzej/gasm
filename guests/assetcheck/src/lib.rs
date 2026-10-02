//! Test guest for asset providers (file-backed assets, folders, case-insensitive
//! lookup, OPFS/Worker mode). Everything it reads is folded into the frames it
//! presents, so runners' video hashes differ if any byte differs.
//!
//! Params:
//! - `read=a,b,c`: read these assets in full at start (any case; logs size + hash)
//! - `stream=name`: read `reads` chunks of `chunk` bytes per frame at pseudo-random offsets
//! - `reads=64`, `chunk=4096`, `frames=120`: then log totals and exit
//! - `list=1`: enumerate assets (asset_count/asset_name), log and hash the names

use gasm::log;

const W: u32 = 32;
const H: u32 = 32;

fn fnv(h: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(h, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

struct Check {
    stream: Option<(String, u64)>,
    reads: u32,
    chunk: Vec<u8>,
    frames: u32,
    frame: u32,
    rng: u64,
    hash: u32,
    bytes: u64,
    fb: Vec<u8>,
}

impl gasm::Game for Check {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let num = |k: &str, d: u32| gasm::param(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let mut hash = 0x811c_9dc5u32;
        let mut bytes = 0u64;
        if gasm::param("list").is_some() {
            let names = gasm::asset_names();
            log!("[assetcheck] {} assets: {}", names.len(), names.join(" "));
            for n in &names {
                hash = fnv(fnv(hash, n.as_bytes()), &[0]);
            }
        }
        for name in gasm::param("read").unwrap_or_default().split(',').filter(|n| !n.is_empty()) {
            match gasm::asset(name) {
                Some(data) => {
                    log!("[assetcheck] {name}: {} bytes fnv={:08x}", data.len(), fnv(0x811c_9dc5, &data));
                    hash = fnv(hash, &data);
                    bytes += data.len() as u64;
                }
                None => {
                    log!("[assetcheck] {name}: missing");
                    hash = fnv(hash, b"missing");
                }
            }
        }
        let stream = gasm::param("stream").map(|name| {
            let size = unsafe { gasm::sys::asset_size(name.as_ptr(), name.len() as u32) };
            log!("[assetcheck] stream {name}: {size} bytes");
            (name, size.max(0) as u64)
        });
        Ok(Check {
            stream,
            reads: num("reads", 64),
            chunk: vec![0; num("chunk", 4096) as usize],
            frames: num("frames", 120),
            frame: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
            hash,
            bytes,
            fb: vec![0; (W * H * 4) as usize],
        })
    }

    fn frame(&mut self) {
        if let Some((name, size)) = &self.stream {
            for _ in 0..self.reads {
                // xorshift64: deterministic offsets on every runner
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 7;
                self.rng ^= self.rng << 17;
                let span = size.saturating_sub(self.chunk.len() as u64).max(1);
                let offset = self.rng % span;
                let n = gasm::asset_read_at(name, offset, &mut self.chunk).unwrap_or(0);
                self.hash = fnv(self.hash, &self.chunk[..n]);
                self.bytes += n as u64;
            }
        }
        // present: the hash state plus the start of the last chunk
        let k = self.fb.len().min(self.chunk.len());
        self.fb[..k].copy_from_slice(&self.chunk[..k]);
        self.fb[..4].copy_from_slice(&self.hash.to_le_bytes());
        gasm::present(&self.fb, W, H, W * 4);
        self.frame += 1;
        if self.frame >= self.frames {
            log!("[assetcheck] done: {} frames, {} bytes read, hash={:08x}", self.frame, self.bytes, self.hash);
            gasm::exit(0);
        }
    }
}

gasm::game!(Check);
