//! Audio output: guest samples -> linear resampler -> ring buffer -> cpal device.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Buffer this much audio before playback starts / after an underrun.
const TARGET_LATENCY_S: f32 = 0.06;
/// Drop the oldest samples when the queue grows past this (guest runs fast).
const MAX_LATENCY_S: f32 = 0.20;

struct Queue {
    samples: VecDeque<f32>, // interleaved stereo at device rate
    primed: bool,
}

pub struct AudioSink {
    queue: Arc<Mutex<Queue>>,
    device_rate: u32,
    src_rate: u32,
    src_channels: u32,
    t: f64,
    prev: [f32; 2],
    _stream: cpal::Stream,
}

impl AudioSink {
    pub fn open() -> Result<AudioSink, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!("unsupported device sample format {:?}", supported.sample_format()));
        }
        let config = supported.config();
        let channels = config.channels as usize;
        let device_rate = config.sample_rate;

        let queue = Arc::new(Mutex::new(Queue { samples: VecDeque::new(), primed: false }));
        let q = queue.clone();
        let target = (device_rate as f32 * TARGET_LATENCY_S) as usize * 2;
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |out: &mut [f32], _| {
                    let mut q = q.lock().unwrap();
                    if !q.primed && q.samples.len() >= target {
                        q.primed = true;
                    }
                    for frame in out.chunks_mut(channels) {
                        let (l, r) = if q.primed && q.samples.len() >= 2 {
                            (q.samples.pop_front().unwrap(), q.samples.pop_front().unwrap())
                        } else {
                            q.primed = false; // underrun: re-buffer
                            (0.0, 0.0)
                        };
                        match frame.len() {
                            1 => frame[0] = 0.5 * (l + r),
                            _ => {
                                frame[0] = l;
                                frame[1] = r;
                                frame[2..].fill(0.0);
                            }
                        }
                    }
                },
                |e| eprintln!("[audio] stream error: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(AudioSink {
            queue,
            device_rate,
            src_rate: 44100,
            src_channels: 2,
            t: 0.0,
            prev: [0.0; 2],
            _stream: stream,
        })
    }

    pub fn device_rate(&self) -> u32 {
        self.device_rate
    }

    pub fn configure(&mut self, rate: u32, channels: u32) {
        self.src_rate = rate;
        self.src_channels = channels;
    }

    /// `bytes` is interleaved little-endian f32 in the configured source format.
    pub fn push_le_f32(&mut self, bytes: &[u8]) {
        let step = self.src_rate as f64 / self.device_rate as f64;
        let ch = self.src_channels as usize;
        let mut out = Vec::with_capacity((bytes.len() / 4 / ch) * 2 * 2);
        for frame in bytes.chunks_exact(4 * ch) {
            let s = |i: usize| f32::from_le_bytes(frame[i * 4..i * 4 + 4].try_into().unwrap());
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
        let max = (self.device_rate as f32 * MAX_LATENCY_S) as usize * 2;
        let target = (self.device_rate as f32 * TARGET_LATENCY_S) as usize * 2;
        let mut q = self.queue.lock().unwrap();
        q.samples.extend(out);
        if q.samples.len() > max {
            let excess = q.samples.len() - target;
            q.samples.drain(..excess & !1);
        }
    }
}
