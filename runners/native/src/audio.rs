//! Audio output: guest samples -> linear resampler -> lock-free ring -> device.
//!
//! [`AudioOut`] is what the host needs; [`AudioSink`] (feature `window`) plays it
//! on the default cpal device in any of its sample formats.

/// Where the host sends `audio_push` samples. `Send`: the host lives in the
/// wasmtime store, which async (`gasm_run`) calls require to be `Send`.
pub trait AudioOut: Send {
    /// Source format of the following pushes (validated by the host).
    fn configure(&mut self, rate: u32, channels: u32);
    /// Interleaved little-endian f32 samples in the configured format.
    fn push_le_f32(&mut self, bytes: &[u8]);
    /// The bus gasm:media streams play into (added after these samples), if this output has one.
    fn media_bus(&self) -> Option<std::sync::Arc<crate::media::MediaBus>> {
        None
    }
}

/// Linear resampler to interleaved stereo (same algorithm as gasm-host.js `Resampler`).
pub struct Resampler {
    dst_rate: u32,
    src_rate: u32,
    channels: u32,
    t: f64,
    prev: [f32; 2],
}

impl Resampler {
    pub fn new(dst_rate: u32) -> Resampler {
        Resampler { dst_rate, src_rate: 44100, channels: 2, t: 0.0, prev: [0.0; 2] }
    }

    pub fn configure(&mut self, rate: u32, channels: u32) {
        self.src_rate = rate;
        self.channels = channels;
    }

    /// Resample `bytes` (LE f32, configured format), appending stereo samples to `out`.
    pub fn process(&mut self, bytes: &[u8], out: &mut Vec<f32>) {
        let step = self.src_rate as f64 / self.dst_rate as f64;
        let ch = self.channels as usize;
        out.reserve(((bytes.len() / 4 / ch) as f64 / step) as usize * 2 + 4);
        for frame in bytes.chunks_exact(4 * ch) {
            let s = |i: usize| f32::from_le_bytes([frame[i * 4], frame[i * 4 + 1], frame[i * 4 + 2], frame[i * 4 + 3]]);
            let cur = if ch == 1 { [s(0), s(0)] } else { [s(0), s(1)] };
            while self.t < 1.0 {
                let t = self.t as f32;
                out.push(self.prev[0] + (cur[0] - self.prev[0]) * t);
                out.push(self.prev[1] + (cur[1] - self.prev[1]) * t);
                self.t += step;
            }
            self.t -= 1.0;
            self.prev = cur;
        }
    }
}

#[cfg(feature = "window")]
pub use sink::{AudioSink, AudioStream};

#[cfg(feature = "window")]
mod sink {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SizedSample};

    use super::{AudioOut, Resampler};
    use crate::media::MediaBus;

    /// Buffer this much audio before playback starts / after an underrun...
    const TARGET_LATENCY_S: f32 = 0.06;
    /// ... more after repeated underruns (a game below its frame rate, a busy system),
    /// up to this: some latency is better than gaps
    const TARGET_STEP_S: f32 = 0.03;
    const TARGET_MAX_S: f32 = 0.15;
    /// Drop the oldest samples when the queue grows past the target plus this (guest runs fast).
    const HEADROOM_S: f32 = 0.14;

    /// Single-producer single-consumer ring of interleaved stereo f32 samples.
    /// The guest thread pushes, the device callback pops; neither ever blocks.
    struct Ring {
        slots: Box<[AtomicU32]>,
        /// total samples written / read (monotonic; index = count % len)
        head: AtomicUsize,
        tail: AtomicUsize,
        /// set by the producer when the queue is too long: the consumer drops
        /// down to the target latency
        trim: AtomicBool,
        /// samples to buffer before playing (grows after repeated underruns)
        target: AtomicUsize,
        /// underruns so far (the consumer counts, the producer adapts the target)
        underruns: AtomicUsize,
        /// trims and samples dropped by them, samples a full ring refused (for `GASM_AUDIO_LOG`)
        trims: AtomicUsize,
        trimmed: AtomicUsize,
        refused: AtomicUsize,
    }

    impl Ring {
        fn new(capacity: usize) -> Ring {
            Ring {
                slots: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                trim: AtomicBool::new(false),
                target: AtomicUsize::new(0),
                underruns: AtomicUsize::new(0),
                trims: AtomicUsize::new(0),
                trimmed: AtomicUsize::new(0),
                refused: AtomicUsize::new(0),
            }
        }

        fn len(&self) -> usize {
            self.head.load(Ordering::Acquire) - self.tail.load(Ordering::Acquire)
        }

        /// Producer: append what fits.
        fn push(&self, samples: &[f32]) {
            let head = self.head.load(Ordering::Relaxed);
            let free = self.slots.len() - (head - self.tail.load(Ordering::Acquire));
            let n = samples.len().min(free) & !1;
            self.refused.fetch_add(samples.len() - n, Ordering::Relaxed);
            for (i, s) in samples[..n].iter().enumerate() {
                self.slots[(head + i) % self.slots.len()].store(s.to_bits(), Ordering::Relaxed);
            }
            self.head.store(head + n, Ordering::Release);
        }

        /// Consumer: one stereo frame, if there is one.
        fn pop(&self) -> Option<(f32, f32)> {
            let tail = self.tail.load(Ordering::Relaxed);
            if self.head.load(Ordering::Acquire) - tail < 2 {
                return None;
            }
            let get = |i: usize| f32::from_bits(self.slots[i % self.slots.len()].load(Ordering::Relaxed));
            let frame = (get(tail), get(tail + 1));
            self.tail.store(tail + 2, Ordering::Release);
            Some(frame)
        }

        /// Consumer: drop the oldest samples down to `keep`.
        fn skip_to(&self, keep: usize) {
            let tail = self.tail.load(Ordering::Relaxed);
            let len = self.head.load(Ordering::Acquire) - tail;
            if len > keep {
                self.trims.fetch_add(1, Ordering::Relaxed);
                self.trimmed.fetch_add((len - keep) & !1, Ordering::Relaxed);
                self.tail.store(tail + ((len - keep) & !1), Ordering::Release);
            }
        }
    }

    pub struct AudioSink {
        ring: Arc<Ring>,
        /// gasm:media's streams, added after the game's samples
        bus: Arc<MediaBus>,
        device_rate: u32,
        resampler: Resampler,
        scratch: Vec<f32>,
        /// samples per second (interleaved stereo)
        per_second: usize,
        /// underruns seen at the last check, and when
        underruns_seen: usize,
        last_underrun: Option<Instant>,
        /// `GASM_AUDIO_LOG=1`: a line a second (queue length, target, pushed, trims, underruns)
        log: Option<AudioLog>,
    }

    struct AudioLog {
        since: Instant,
        pushed: usize,
        min: usize,
        max: usize,
    }

    /// The device stream feeding from an [`AudioSink`]: keep it alive while
    /// playing. Separate because cpal streams aren't `Send` (the sink goes into
    /// the host, see [`AudioOut`]). [`AudioStream::check`] rebuilds it when it broke or
    /// the default output device changed.
    pub struct AudioStream {
        stream: cpal::Stream,
        ring: Arc<Ring>,
        bus: Arc<MediaBus>,
        device_name: Option<String>,
        device_rate: u32,
        broken: Arc<AtomicBool>,
        last_check: Instant,
    }

    /// The device callback's half of the ring: `frames` stereo frames into `mix`. While
    /// buffering (not `primed`) it plays silence and leaves the samples queued, so the
    /// queue can fill up to the target; it starts playing once it has.
    fn fill(ring: &Ring, primed: &mut bool, frames: usize, mix: &mut Vec<f32>) {
        let target = ring.target.load(Ordering::Relaxed);
        if ring.trim.swap(false, Ordering::AcqRel) {
            ring.skip_to(target);
        }
        if !*primed && ring.len() >= target {
            *primed = true;
        }
        mix.clear();
        mix.resize(frames * 2, 0.0);
        for f in 0..frames {
            if !*primed {
                break; // the rest of the block stays silent
            }
            match ring.pop() {
                Some((l, r)) => (mix[f * 2], mix[f * 2 + 1]) = (l, r),
                None => {
                    ring.underruns.fetch_add(1, Ordering::Relaxed);
                    *primed = false; // underrun: re-buffer
                }
            }
        }
    }

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        ring: Arc<Ring>,
        broken: Arc<AtomicBool>,
        bus: Arc<MediaBus>,
    ) -> Result<cpal::Stream, cpal::Error> {
        let channels = config.channels as usize;
        let mut primed = false;
        // the block in stereo f32: the game's samples, then gasm:media's streams on top
        let mut mix: Vec<f32> = Vec::new();
        device.build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                let frames = out.len() / channels.max(1);
                fill(&ring, &mut primed, frames, &mut mix);
                bus.mix(&mut mix);
                for (f, frame) in out.chunks_mut(channels).enumerate() {
                    let (l, r) = (mix[f * 2].clamp(-1.0, 1.0), mix[f * 2 + 1].clamp(-1.0, 1.0));
                    match frame.len() {
                        1 => frame[0] = T::from_sample(0.5 * (l + r)),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            for s in &mut frame[2..] {
                                *s = T::from_sample(0.0);
                            }
                        }
                    }
                }
            },
            move |e| {
                // e.g. the device went away: the runner rebuilds the stream (AudioStream::check)
                eprintln!("[audio] stream error: {e}");
                broken.store(true, Ordering::Release);
            },
            None,
        )
    }

    /// The default output device's stream on `ring`, and the device's name and rate.
    fn open_stream(ring: &Arc<Ring>, broken: &Arc<AtomicBool>, bus: &Arc<MediaBus>) -> Result<(cpal::Stream, Option<String>, u32), String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        // the device's id tells a change of device; its name goes to the log
        let name = device.id().ok().map(|i| i.to_string());
        if let Ok(d) = device.description() {
            eprintln!("[audio] output: {}", d.name());
        }
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config = supported.config();
        let (r, b, m) = (ring.clone(), broken.clone(), bus.clone());
        use cpal::SampleFormat as F;
        let stream = match format {
            F::F32 => build::<f32>(&device, &config, r, b, m),
            F::F64 => build::<f64>(&device, &config, r, b, m),
            F::I16 => build::<i16>(&device, &config, r, b, m),
            F::U16 => build::<u16>(&device, &config, r, b, m),
            F::I32 => build::<i32>(&device, &config, r, b, m),
            F::U32 => build::<u32>(&device, &config, r, b, m),
            F::I8 => build::<i8>(&device, &config, r, b, m),
            F::U8 => build::<u8>(&device, &config, r, b, m),
            other => return Err(format!("unsupported device sample format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, name, config.sample_rate))
    }

    impl AudioStream {
        /// Call now and then (the window runner: every frame; it checks once a second):
        /// rebuilds the stream if it broke or the default output device changed. A device
        /// with another rate than the first is kept as is (the sink resamples to that one).
        pub fn check(&mut self) {
            if self.last_check.elapsed() < Duration::from_secs(1) {
                return;
            }
            self.last_check = Instant::now();
            let now = cpal::default_host().default_output_device().and_then(|d| d.id().ok().map(|i| i.to_string()));
            let changed = now.is_some() && now != self.device_name;
            if !self.broken.load(Ordering::Acquire) && !changed {
                return;
            }
            self.broken.store(false, Ordering::Release);
            match open_stream(&self.ring, &self.broken, &self.bus) {
                Ok((stream, name, rate)) if rate == self.device_rate => (self.stream, self.device_name) = (stream, name),
                Ok((_, name, rate)) => {
                    eprintln!("[audio] the new output runs at {rate} Hz, not {} Hz: staying on the old one", self.device_rate);
                    self.device_name = name;
                }
                Err(e) => eprintln!("[audio] couldn't reopen the output: {e}"),
            }
        }
    }

    impl AudioSink {
        pub fn open() -> Result<(AudioSink, AudioStream), String> {
            let device_rate = cpal::default_host()
                .default_output_device()
                .ok_or("no audio output device")?
                .default_output_config()
                .map_err(|e| e.to_string())?
                .config()
                .sample_rate;
            let per_second = device_rate as usize * 2;
            let max = (per_second as f32 * (TARGET_MAX_S + HEADROOM_S)) as usize;
            let ring = Arc::new(Ring::new(max * 2));
            ring.target.store((per_second as f32 * TARGET_LATENCY_S) as usize & !1, Ordering::Relaxed);
            let broken = Arc::new(AtomicBool::new(false));
            let bus = MediaBus::new(device_rate);
            let (stream, device_name, device_rate) = open_stream(&ring, &broken, &bus)?;
            let sink = AudioSink {
                ring: ring.clone(), bus: bus.clone(), device_rate, resampler: Resampler::new(device_rate), scratch: Vec::new(),
                per_second, underruns_seen: 0, last_underrun: None,
                log: std::env::var_os("GASM_AUDIO_LOG").filter(|v| !v.is_empty() && v != "0").map(|_| AudioLog { since: Instant::now(), pushed: 0, min: usize::MAX, max: 0 }),
            };
            Ok((sink, AudioStream { stream, ring, bus, device_name, device_rate, broken, last_check: Instant::now() }))
        }

        pub fn device_rate(&self) -> u32 {
            self.device_rate
        }
    }

    impl AudioOut for AudioSink {
        fn configure(&mut self, rate: u32, channels: u32) {
            self.resampler.configure(rate, channels);
        }

        fn media_bus(&self) -> Option<Arc<MediaBus>> {
            Some(self.bus.clone())
        }

        fn push_le_f32(&mut self, bytes: &[u8]) {
            self.scratch.clear();
            self.resampler.process(bytes, &mut self.scratch);
            self.ring.push(&self.scratch);
            // two underruns within 5 s: buffer more (30 ms at a time, up to 150 ms)
            let underruns = self.ring.underruns.load(Ordering::Relaxed);
            if underruns != self.underruns_seen {
                self.underruns_seen = underruns;
                let again = self.last_underrun.is_some_and(|t| t.elapsed() < Duration::from_secs(5));
                self.last_underrun = Some(Instant::now());
                let (target, max) = (self.ring.target.load(Ordering::Relaxed), (self.per_second as f32 * TARGET_MAX_S) as usize);
                if again && target < max {
                    let more = (target + (self.per_second as f32 * TARGET_STEP_S) as usize).min(max) & !1;
                    self.ring.target.store(more, Ordering::Relaxed);
                    eprintln!("[audio] underruns: buffering {} ms", more * 1000 / self.per_second);
                }
            }
            let target = self.ring.target.load(Ordering::Relaxed);
            if let Some(log) = &mut self.log {
                let len = self.ring.len();
                (log.pushed, log.min, log.max) = (log.pushed + self.scratch.len(), log.min.min(len), log.max.max(len));
                if log.since.elapsed() >= Duration::from_secs(1) {
                    let ms = |n: usize| n * 1000 / self.per_second;
                    let r = &self.ring;
                    eprintln!(
                        "[audio] queue {}-{} ms (target {} ms), pushed {} ms in {} ms; total: trims {} ({} ms dropped), refused {} ms, underruns {}",
                        ms(log.min), ms(log.max), ms(target), ms(log.pushed), log.since.elapsed().as_millis(),
                        r.trims.load(Ordering::Relaxed), ms(r.trimmed.load(Ordering::Relaxed)),
                        ms(r.refused.load(Ordering::Relaxed)), r.underruns.load(Ordering::Relaxed),
                    );
                    *log = AudioLog { since: Instant::now(), pushed: 0, min: usize::MAX, max: 0 };
                }
            }
            if self.ring.len() > target + (self.per_second as f32 * HEADROOM_S) as usize {
                self.ring.trim.store(true, Ordering::Release);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A game pushing in real time, a block at a time, must start playing: buffering
        /// used to pop (and throw away) the samples it waited for, so the queue never
        /// reached the target and the game stayed silent.
        #[test]
        fn buffering_keeps_the_samples() {
            let ring = Ring::new(4096);
            ring.target.store(600, Ordering::Relaxed); // 300 frames
            let (mut primed, mut mix) = (false, Vec::new());
            let block: Vec<f32> = (0..200).map(|i| i as f32 + 1.0).collect(); // 100 frames
            let mut heard = Vec::new();
            for _ in 0..10 {
                ring.push(&block);
                fill(&ring, &mut primed, 100, &mut mix);
                heard.extend(mix.iter().copied().filter(|&s| s != 0.0));
            }
            // once primed, every pushed sample comes out, in order
            assert!(primed);
            assert_eq!(heard.len() + ring.len(), block.len() * 10);
            assert_eq!(&heard[..200], &block[..]);
            assert_eq!(ring.underruns.load(Ordering::Relaxed), 0);
        }

        #[test]
        fn underrun_rebuffers_without_dropping() {
            let ring = Ring::new(4096);
            ring.target.store(4, Ordering::Relaxed);
            let (mut primed, mut mix) = (false, Vec::new());
            ring.push(&[1.0, 1.0, 2.0, 2.0]);
            fill(&ring, &mut primed, 3, &mut mix); // two frames, then an underrun
            assert_eq!(mix, [1.0, 1.0, 2.0, 2.0, 0.0, 0.0]);
            assert!(!primed);
            assert_eq!(ring.underruns.load(Ordering::Relaxed), 1);
            ring.push(&[3.0, 3.0]);
            fill(&ring, &mut primed, 1, &mut mix); // below the target: silence, kept
            assert_eq!((mix.as_slice(), ring.len()), (&[0.0, 0.0][..], 2));
            ring.push(&[4.0, 4.0]);
            fill(&ring, &mut primed, 2, &mut mix);
            assert_eq!(mix, [3.0, 3.0, 4.0, 4.0]);
        }
    }
}
