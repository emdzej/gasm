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
                self.tail.store(tail + ((len - keep) & !1), Ordering::Release);
            }
        }
    }

    pub struct AudioSink {
        ring: Arc<Ring>,
        device_rate: u32,
        resampler: Resampler,
        scratch: Vec<f32>,
        /// samples per second (interleaved stereo)
        per_second: usize,
        /// underruns seen at the last check, and when
        underruns_seen: usize,
        last_underrun: Option<Instant>,
    }

    /// The device stream feeding from an [`AudioSink`]: keep it alive while
    /// playing. Separate because cpal streams aren't `Send` (the sink goes into
    /// the host, see [`AudioOut`]). [`AudioStream::check`] rebuilds it when it broke or
    /// the default output device changed.
    pub struct AudioStream {
        stream: cpal::Stream,
        ring: Arc<Ring>,
        device_name: Option<String>,
        device_rate: u32,
        broken: Arc<AtomicBool>,
        last_check: Instant,
    }

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        ring: Arc<Ring>,
        broken: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::Error> {
        let channels = config.channels as usize;
        let mut primed = false;
        device.build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                let target = ring.target.load(Ordering::Relaxed);
                if ring.trim.swap(false, Ordering::AcqRel) {
                    ring.skip_to(target);
                }
                if !primed && ring.len() >= target {
                    primed = true;
                }
                for frame in out.chunks_mut(channels) {
                    let (l, r) = match ring.pop().filter(|_| primed) {
                        Some(s) => s,
                        None => {
                            if primed {
                                ring.underruns.fetch_add(1, Ordering::Relaxed);
                            }
                            primed = false; // underrun: re-buffer
                            (0.0, 0.0)
                        }
                    };
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
    fn open_stream(ring: &Arc<Ring>, broken: &Arc<AtomicBool>) -> Result<(cpal::Stream, Option<String>, u32), String> {
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
        let (r, b) = (ring.clone(), broken.clone());
        use cpal::SampleFormat as F;
        let stream = match format {
            F::F32 => build::<f32>(&device, &config, r, b),
            F::F64 => build::<f64>(&device, &config, r, b),
            F::I16 => build::<i16>(&device, &config, r, b),
            F::U16 => build::<u16>(&device, &config, r, b),
            F::I32 => build::<i32>(&device, &config, r, b),
            F::U32 => build::<u32>(&device, &config, r, b),
            F::I8 => build::<i8>(&device, &config, r, b),
            F::U8 => build::<u8>(&device, &config, r, b),
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
            match open_stream(&self.ring, &self.broken) {
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
            let (stream, device_name, device_rate) = open_stream(&ring, &broken)?;
            let sink = AudioSink {
                ring: ring.clone(), device_rate, resampler: Resampler::new(device_rate), scratch: Vec::new(),
                per_second, underruns_seen: 0, last_underrun: None,
            };
            Ok((sink, AudioStream { stream, ring, device_name, device_rate, broken, last_check: Instant::now() }))
        }

        pub fn device_rate(&self) -> u32 {
            self.device_rate
        }
    }

    impl AudioOut for AudioSink {
        fn configure(&mut self, rate: u32, channels: u32) {
            self.resampler.configure(rate, channels);
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
            if self.ring.len() > target + (self.per_second as f32 * HEADROOM_S) as usize {
                self.ring.trim.store(true, Ordering::Release);
            }
        }
    }
}
