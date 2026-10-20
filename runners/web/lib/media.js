// gasm:media in the JS host (design/media.md): audio streams played by the page, not
// the guest. The rules (https, 4 streams, the permission, handles, errors) are here, the
// same as gasm-run's; the playing is a backend's: ElementMedia (an <audio> element per
// stream, in a page) or, in Worker mode, a relay to the page's. Without a backend
// (gasm-headless, Node) every open is refused, saying why.

export const CONNECTING = 0, PLAYING = 1, PAUSED = 2, FAILED = 3, ENDED = 4;
export const CAN_VOLUME = 1, CAN_LOWPASS = 2, CAN_METADATA = 4;
export const MAX_STREAMS = 4;
const MAX_URL = 2048;
/** The consent subject (the manifest's "media": true, asked up front). */
export const MEDIA_SUBJECT = 'media';

/** `{"url": "https://..."}` -> the URL, or throws why it's refused (the same rules as gasm-run). */
export function parseMediaDesc(json) {
  let v;
  try { v = JSON.parse(json); } catch (e) { throw new Error(`not valid JSON: ${e.message}`); }
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('not a JSON object');
  if (typeof v.url !== 'string') throw new Error('no "url"');
  const url = v.url;
  if (new TextEncoder().encode(url).length > MAX_URL) throw new Error(`the URL is longer than ${MAX_URL} bytes`);
  if (!url.startsWith('https://')) throw new Error('only https:// streams');
  // eslint-disable-next-line no-control-regex
  if (/[\u0000- \u007f]/.test(url)) throw new Error(`not a URL: ${JSON.stringify(url)}`);
  try { if (!new URL(url).hostname) throw new Error(); } catch { throw new Error(`not a URL: ${JSON.stringify(url)}`); }
  return url;
}

/** A page can't resolve names, so only the obvious: localhost and private or local IP literals. */
export function isLocalHost(host) {
  const h = host.replace(/^\[|\]$/g, '').toLowerCase();
  if (h === 'localhost' || h.endsWith('.localhost')) return true;
  const v4 = h.match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)$/);
  if (v4) {
    const [a, b] = [Number(v4[1]), Number(v4[2])];
    return a === 0 || a === 10 || a === 127 || (a === 100 && (b & 0xc0) === 64) || (a === 169 && b === 254) || (a === 172 && b >= 16 && b < 32) || (a === 192 && b === 168) || a >= 224;
  }
  if (h.includes(':')) return h === '::1' || h === '::' || /^f[cd]/.test(h) || /^fe[89ab]/.test(h) || /^ff/.test(h) || /^::ffff:/.test(h);
  return false;
}

/**
 * The game's streams. `allow`: false, true (any public host) or a list of hosts (also
 * how a private address is reached); without it the manifest must declare "media" and
 * the player answer `consent.check("media")`.
 */
export class MediaStreams {
  constructor({ allow = false, consent = null, backend = null, log = () => {} } = {}) {
    this.allow = allow === true ? [] : Array.isArray(allow) ? allow.map((h) => String(h).toLowerCase()) : null;
    this.consent = consent;
    this.backend = backend;
    this.log = log;
    this.declared = false;    // the manifest's "media" (the host sets it when it loads the game)
    this.streams = new Map(); // handle -> stream | null (closed)
    this.next = 1;
    this.lastRefusal = '';
  }

  refuse(why) {
    this.log(`[gasm] media: refused: ${why}`);
    this.lastRefusal = why;
    return -1;
  }

  /** Why a URL may not be fetched, or null. */
  urlRefusal(url) {
    const host = new URL(url).hostname.toLowerCase();
    const listed = this.allow?.includes(host) || this.allow?.includes(host.replace(/^\[|\]$/g, ''));
    if (this.allow?.length && !listed) return `${host} isn't in the page's list of media hosts`;
    if (!listed && isLocalHost(host)) return `${host} is a private or local address: list it to reach it`;
    return null;
  }

  open(desc) {
    let url;
    try { url = parseMediaDesc(desc); } catch (e) { return this.refuse(e.message); }
    if (!this.backend) return this.refuse("this runner can't play audio streams (no page to play them in)");
    if (this.allow === null && !this.declared) return this.refuse("this game's manifest doesn't ask to play media (\"media\": true)");
    if (this.allow === null && !this.consent) return this.refuse('media streams need the page to allow them here');
    if ([...this.streams.values()].filter(Boolean).length >= MAX_STREAMS) return this.refuse(`${MAX_STREAMS} streams are open already`);
    const h = this.next++;
    const s = { url, info: { state: CONNECTING, error: '', caps: 0, position: 0, duration: -1, metadata: { name: null, title: null, codec: null, bitrate: null }, version: 0 }, play: null, want: false, gain: 1, hz: 0 };
    this.streams.set(h, s);
    // the player's answer first (asked up front from the manifest; connecting meanwhile)
    const why = this.urlRefusal(url);
    if (why) return this.fail(s, why), h;
    const answer = this.allow !== null ? true : this.consent.check(MEDIA_SUBJECT);
    const start = (yes) => {
      if (this.streams.get(h) !== s) return;
      if (!yes) return this.fail(s, 'the player said no to internet audio');
      s.play = this.backend.create(url, (info) => {
        if (this.streams.get(h) !== s) return;
        if (info.metadata && JSON.stringify(info.metadata) !== JSON.stringify(s.info.metadata)) s.info.version = (s.info.version + 1) >>> 0;
        Object.assign(s.info, info);
      });
      s.play.setVolume(s.gain);
      s.play.setLowpass(s.hz);
      if (s.want) s.play.play();
    };
    if (answer === true || answer === false) start(answer); else answer.then(start, () => start(false));
    return h;
  }

  fail(s, why) {
    this.log(`[gasm] media: ${why}`);
    Object.assign(s.info, { state: FAILED, error: why });
  }

  get(h) {
    if (!this.streams.has(h)) throw new Error(`gasm:media: ${h} is not a stream handle`);
    return this.streams.get(h);
  }

  play(h) { const s = this.get(h); if (s) { s.want = true; s.play?.play(); } }
  pause(h) { const s = this.get(h); if (s) { s.want = false; s.play?.pause(); } }
  close(h) {
    const s = this.get(h);
    if (s) { s.play?.close(); this.streams.set(h, null); }
  }
  setVolume(h, gain) {
    const s = this.get(h);
    if (s) { s.gain = Number.isNaN(gain) ? 0 : Math.min(1, Math.max(0, gain)); s.play?.setVolume(s.gain); }
  }
  setLowpass(h, hz) {
    const s = this.get(h);
    if (s) { s.hz = Number.isNaN(hz) || hz <= 0 ? 0 : Math.min(20000, Math.max(20, hz)); s.play?.setLowpass(s.hz); }
  }
  info(h, closed) { const s = this.get(h); return s ? s.info : closed; }
  caps(h) { return this.info(h, { caps: 0 }).caps; }
  state(h) { return this.info(h, { state: FAILED }).state; }
  error(h) { return h === -1 ? this.lastRefusal : this.info(h, { error: 'closed' }).error; }
  position(h) { return this.info(h, { position: 0 }).position; }
  duration(h) { return this.info(h, { duration: -1 }).duration; }
  metadata(h) {
    const m = this.info(h, { metadata: { name: null, title: null, codec: null, bitrate: null } }).metadata;
    return JSON.stringify({ bitrate: m.bitrate, codec: m.codec, name: m.name, title: m.title });
  }
  version(h) { return this.info(h, { version: 0 }).version; }

  shutdown() { for (const s of this.streams.values()) s?.play?.close(); this.streams.clear(); }
}

/**
 * Streams on <audio> elements (a page). `create(url, report)` -> controls; `report(info)`
 * with what changed. With CORS the element plays through Web Audio (gain and a
 * low-pass); a stream without it fails to load that way, and plays again plainly
 * (element volume only). Browsers have no ICY titles: metadata stays empty.
 */
export class ElementMedia {
  constructor() { this.ctx = null; }

  context() {
    if (!this.ctx) {
      const AC = globalThis.AudioContext ?? globalThis.webkitAudioContext;
      this.ctx = AC ? new AC() : null;
    }
    return this.ctx;
  }

  create(url, report) {
    const ctl = { closed: false, want: false, gain: 1, hz: 0, el: null, cors: true, graph: null, pausedAt: 0, live: false, timer: 0 };
    const caps = () => (ctl.graph ? CAN_VOLUME | CAN_LOWPASS : volumeWorks() ? CAN_VOLUME : 0);
    const apply = () => {
      if (ctl.graph) {
        ctl.graph.gain.gain.setTargetAtTime(ctl.gain, ctl.graph.ctx.currentTime, 0.007);
        ctl.graph.filter.frequency.setTargetAtTime(ctl.hz > 0 ? ctl.hz : ctl.graph.ctx.sampleRate / 2, ctl.graph.ctx.currentTime, 0.007);
        if (ctl.el) ctl.el.volume = 1;
      } else if (ctl.el) {
        ctl.el.volume = ctl.gain;
      }
    };
    const element = () => {
      const el = new Audio();
      el.preload = 'none';
      if (ctl.cors) el.crossOrigin = 'anonymous';
      el.addEventListener('playing', () => {
        ctl.live = !Number.isFinite(el.duration);
        if (ctl.cors && !ctl.graph) {
          const ctx = this.context();
          if (ctx) {
            const src = ctx.createMediaElementSource(el);
            const filter = ctx.createBiquadFilter();
            filter.type = 'lowpass';
            filter.Q.value = Math.SQRT1_2;
            const gain = ctx.createGain();
            src.connect(filter).connect(gain).connect(ctx.destination);
            ctl.graph = { ctx, filter, gain };
            ctx.resume?.();
          }
        }
        apply();
        report({ state: PLAYING, error: '', caps: caps(), duration: ctl.live ? -1 : el.duration, metadata: { name: null, title: null, codec: codecOf(url), bitrate: null } });
      });
      el.addEventListener('waiting', () => { if (ctl.want) report({ state: CONNECTING }); });
      el.addEventListener('pause', () => { if (!ctl.want && !el.ended) report({ state: PAUSED }); });
      el.addEventListener('ended', () => report({ state: ENDED, position: el.currentTime }));
      el.addEventListener('error', () => {
        if (ctl.closed) return;
        // with CORS refused, the stream doesn't load at all: once more without (volume only)
        if (ctl.cors) {
          ctl.cors = false;
          el.removeAttribute('src');
          ctl.el = element();
          if (ctl.want) start();
          return;
        }
        const e = el.error;
        report({ state: FAILED, error: `${url}: ${e?.message || ['', 'aborted', 'a network error', "can't decode it", 'not a stream the browser can play'][e?.code ?? 0] || 'failed'}` });
      });
      return el;
    };
    const start = () => {
      const el = ctl.el;
      // a live stream paused for long: from now, not from the stale buffer
      if (ctl.live && ctl.pausedAt && performance.now() - ctl.pausedAt > 10_000) el.removeAttribute('src');
      if (!el.getAttribute('src')) el.src = url;
      ctl.pausedAt = 0;
      apply();
      el.play().catch((e) => {
        if (ctl.closed || e.name === 'AbortError') return;
        report({ state: FAILED, error: e.name === 'NotAllowedError' ? 'the browser wants a click or key before it plays audio' : `${url}: ${e.message}` });
      });
      if (this.ctx?.state === 'suspended') this.ctx.resume();
    };
    ctl.el = element();
    ctl.timer = setInterval(() => { if (ctl.el && !ctl.closed) report({ position: ctl.el.currentTime }); }, 250);
    return {
      play: () => { if (ctl.closed) return; ctl.want = true; report({ state: CONNECTING }); start(); },
      pause: () => { if (ctl.closed) return; ctl.want = false; ctl.pausedAt = performance.now(); ctl.el.pause(); report({ state: PAUSED }); },
      close: () => {
        ctl.closed = true;
        clearInterval(ctl.timer);
        ctl.el.pause();
        ctl.el.removeAttribute('src');
        ctl.el.load();
      },
      setVolume: (g) => { ctl.gain = g; apply(); },
      setLowpass: (hz) => { ctl.hz = hz; apply(); },
    };
  }
}

/** iOS Safari ignores element volume (it reads back 1). */
function volumeWorks() {
  if (volumeWorks.known !== undefined) return volumeWorks.known;
  const a = new Audio();
  a.volume = 0.5;
  return (volumeWorks.known = a.volume === 0.5);
}

function codecOf(url) {
  const ext = new URL(url).pathname.split('.').pop().toLowerCase();
  return { mp3: 'mp3', aac: 'aac', m4a: 'aac', ogg: 'vorbis', oga: 'vorbis', opus: 'opus', flac: 'flac' }[ext] ?? null;
}
