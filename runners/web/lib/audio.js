// Streaming linear resampler to interleaved stereo (same algorithm as
// runners/native/src/audio.rs Resampler).
export class Resampler {
  constructor(dstRate) { this.dstRate = dstRate; this.t = 0; this.l = 0; this.r = 0; }
  /** Resample interleaved `samples` (1 or 2 channels). Returns a new Float32Array
   *  (safe to transfer to an AudioWorklet). */
  process(samples, srcRate, channels) {
    const step = srcRate / this.dstRate;
    const frames = samples.length / channels;
    const out = new Float32Array(Math.ceil((frames + 1) / step) * 2 + 4);
    let n = 0, t = this.t, pl = this.l, pr = this.r;
    for (let i = 0; i < frames; i++) {
      const l = samples[i * channels], r = channels === 2 ? samples[i * channels + 1] : l;
      while (t < 1) {
        out[n++] = pl + (l - pl) * t;
        out[n++] = pr + (r - pr) * t;
        t += step;
      }
      t -= 1;
      pl = l; pr = r;
    }
    this.t = t; this.l = pl; this.r = pr;
    return n === out.length ? out : out.slice(0, n);
  }
}
