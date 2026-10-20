//! gasm:media: audio streams (internet radio, tracks) the runner fetches, decodes and
//! plays itself, mixed after the guest's audio (design/media.md). The guest only
//! controls them and reads their state and metadata; it never gets their samples.
//!
//! Each stream has two threads: the connection (HTTP, redirects checked against the
//! policy, ICY metadata cut out of the audio) hands chunks to the decoder (symphonia),
//! which resamples to the output rate and fills the stream's buffer (about 2 s). The
//! [`MediaBus`] drains every buffer: the window runner's audio callback adds them to the
//! game's samples (with each stream's volume and low-pass), headless runs drain them on
//! a clock, silently, so state, position and metadata behave the same.

use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::audio::Resampler;
use crate::net::url_host;

pub const CONNECTING: u32 = 0;
pub const PLAYING: u32 = 1;
pub const PAUSED: u32 = 2;
pub const FAILED: u32 = 3;
pub const ENDED: u32 = 4;
pub const CAN_VOLUME: u32 = 1;
pub const CAN_LOWPASS: u32 = 2;
pub const CAN_METADATA: u32 = 4;

/// Streams open at once, per game.
pub const MAX_STREAMS: usize = 4;
/// Longest URL in a description.
const MAX_URL: usize = 2048;
/// Longest ICY title kept (bytes).
const MAX_TITLE: usize = 512;
/// What a stream buffers ahead, in seconds.
const BUFFER_S: f64 = 2.0;
/// The buffer it waits for before it counts as playing.
const START_S: f64 = 0.25;
/// No data for this long: the connection stalled (a live stream reconnects).
const STALL: Duration = Duration::from_secs(15);
/// A live stream paused for this long drops its connection (and reconnects on play).
const PAUSE_DROP: Duration = Duration::from_secs(10);
/// Tries to reconnect a dropped live stream (1, 2, 4 s apart) before it fails.
const RECONNECTS: u32 = 3;
const MAX_REDIRECTS: u32 = 5;

// ---- the permission -------------------------------------------------------------------

/// Who may open streams: `--allow-media` (any public host, or a list), else the player's
/// yes when the manifest declares `"media": true` (consent subject `media`, asked up
/// front). Private and loopback addresses are reached only when listed.
#[derive(Clone, Debug, Default)]
pub struct MediaPolicy {
    /// `--allow-media`: None = not given; Some(empty) = any public host
    pub allowed: Option<Vec<String>>,
    /// the manifest's `"media": true`
    pub declared: bool,
    pub consent: Option<crate::consent::Consent>,
}

pub const CONSENT_SUBJECT: &str = "media";

impl MediaPolicy {
    pub fn new(allowed: Option<Vec<String>>) -> MediaPolicy {
        MediaPolicy { allowed: allowed.map(|h| h.into_iter().map(|h| h.to_ascii_lowercase()).collect()), declared: false, consent: None }
    }

    fn listed(&self, host: &str) -> bool {
        self.allowed.as_ref().is_some_and(|l| l.iter().any(|h| h == host))
    }

    /// Whether the game may have streams at all (checked when it opens one): Err(why) if not.
    fn may_open(&self) -> Result<(), String> {
        if self.allowed.is_some() {
            return Ok(());
        }
        if !self.declared {
            return Err("this game's manifest doesn't ask to play media (\"media\": true), and --allow-media isn't given".into());
        }
        if self.consent.is_none() {
            return Err("media streams need --allow-media here".into());
        }
        Ok(())
    }

    /// The player's answer for this game: Ok(true) yes, Ok(false) still asking, Err(why) no.
    fn answered(&self) -> Result<bool, String> {
        if self.allowed.is_some() {
            return Ok(true);
        }
        match self.consent.as_ref().map(|c| c.lock().unwrap().check(CONSENT_SUBJECT)) {
            Some(Some(true)) => Ok(true),
            Some(Some(false)) => Err("the player said no to internet audio".into()),
            Some(None) => Ok(false),
            None => Err("media streams need --allow-media here".into()),
        }
    }

    /// Whether `url` may be fetched (each hop of a redirect): https, a listed host if
    /// --allow-media has a list, and a public address unless the host is listed.
    pub fn check_url(&self, url: &str) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err(format!("{url}: only https streams"));
        }
        let host = url_host(url).ok_or_else(|| format!("{url}: no host"))?;
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        if let Some(list) = &self.allowed {
            if !list.is_empty() && !self.listed(&host) && !self.listed(bare) {
                return Err(format!("{host} isn't in --allow-media's list"));
            }
        }
        if self.listed(&host) || self.listed(bare) {
            return Ok(());
        }
        let port = url_port(url);
        let addrs: Vec<IpAddr> = match bare.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => (bare, port).to_socket_addrs().map_err(|e| format!("{host}: {e}"))?.map(|a| a.ip()).collect(),
        };
        match addrs.iter().find(|ip| !is_public(**ip)) {
            Some(ip) => Err(format!("{host} is a private or local address ({ip}): list it in --allow-media to reach it")),
            None => Ok(()),
        }
    }
}

fn url_port(url: &str) -> u16 {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let after = authority.rsplit_once(']').map_or(authority, |(_, a)| a);
    after.rsplit_once(':').and_then(|(_, p)| p.parse().ok()).unwrap_or(443)
}

/// Not the player's own machine or network: no loopback, private, link-local, CGNAT,
/// unspecified, broadcast, multicast or documentation addresses.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_multicast()
                || v.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                || o[0] >= 240)
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v.segments();
            !(v.is_loopback() || v.is_unspecified() || v.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || s[0] == 0x2001 && s[1] == 0xdb8)
        }
    }
}

/// A stream's description: `{"url": "https://..."}`.
pub fn parse_desc(json: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not valid JSON: {e}"))?;
    let o = v.as_object().ok_or("not a JSON object")?;
    let url = o.get("url").and_then(|u| u.as_str()).ok_or("no \"url\"")?;
    if url.len() > MAX_URL {
        return Err(format!("the URL is longer than {MAX_URL} bytes"));
    }
    if !url.starts_with("https://") {
        return Err("only https:// streams".into());
    }
    if url.bytes().any(|b| b <= b' ' || b == 0x7f) || url_host(url).is_none() {
        return Err(format!("not a URL: {url:?}"));
    }
    Ok(url.to_owned())
}

// ---- the bus: buffers drained by the output (or a clock) ------------------------------

/// What one stream plays: its buffer (interleaved stereo at the bus's rate) and the
/// controls the output reads.
pub struct Voice {
    buf: Mutex<VecDeque<f32>>,
    /// target gain, f32 bits
    gain: AtomicU32,
    /// low-pass frequency, f32 bits (0 = off)
    lowpass: AtomicU32,
    playing: AtomicBool,
    /// stereo frames played (position)
    played: AtomicU64,
    /// the output's own state: the smoothed gain and the filter
    dsp: Mutex<Dsp>,
}

#[derive(Default)]
struct Dsp {
    gain: f32,
    hz: f32,
    /// biquad coefficients (b0, b1, b2, a1, a2) and per-channel state (x1, x2, y1, y2)
    k: [f32; 5],
    s: [[f32; 4]; 2],
}

impl Voice {
    fn new() -> Voice {
        Voice {
            buf: Mutex::new(VecDeque::new()),
            gain: AtomicU32::new(1.0f32.to_bits()),
            lowpass: AtomicU32::new(0),
            playing: AtomicBool::new(false),
            played: AtomicU64::new(0),
            dsp: Mutex::new(Dsp { gain: 1.0, ..Default::default() }),
        }
    }

    fn buffered(&self) -> usize {
        self.buf.lock().unwrap().len() / 2
    }
}

/// Every open stream's voice, drained at `rate` (the output's).
pub struct MediaBus {
    pub rate: u32,
    voices: Mutex<Vec<Arc<Voice>>>,
}

impl MediaBus {
    pub fn new(rate: u32) -> Arc<MediaBus> {
        Arc::new(MediaBus { rate, voices: Mutex::new(Vec::new()) })
    }

    /// A bus no output drains: a thread plays it to nowhere in real time (headless runs,
    /// a muted window), so streams behave as when heard.
    pub fn silent() -> Arc<MediaBus> {
        let bus = MediaBus::new(48_000);
        let b = Arc::downgrade(&bus);
        std::thread::Builder::new()
            .name("gasm-media-clock".into())
            .spawn(move || {
                let mut last = Instant::now();
                let mut owed = 0.0f64;
                let mut scratch = Vec::new();
                while let Some(bus) = b.upgrade() {
                    std::thread::sleep(Duration::from_millis(20));
                    owed += last.elapsed().as_secs_f64() * bus.rate as f64;
                    last = Instant::now();
                    let n = owed as usize;
                    owed -= n as f64;
                    scratch.clear();
                    scratch.resize(n * 2, 0.0);
                    bus.mix(&mut scratch);
                }
            })
            .expect("media clock thread");
        bus
    }

    fn add(&self, v: Arc<Voice>) {
        self.voices.lock().unwrap().push(v);
    }

    fn remove(&self, v: &Arc<Voice>) {
        self.voices.lock().unwrap().retain(|x| !Arc::ptr_eq(x, v));
    }

    /// Add every playing stream to `out` (interleaved stereo at `rate`): called by the
    /// output for each block it plays. Never waits: a block when the list is being
    /// changed goes without the streams.
    pub fn mix(&self, out: &mut [f32]) {
        let Ok(voices) = self.voices.try_lock() else { return };
        for v in voices.iter() {
            if !v.playing.load(Ordering::Acquire) {
                continue;
            }
            let Ok(mut dsp) = v.dsp.try_lock() else { continue };
            let target = f32::from_bits(v.gain.load(Ordering::Relaxed));
            let hz = f32::from_bits(v.lowpass.load(Ordering::Relaxed));
            if hz != dsp.hz {
                dsp.hz = hz;
                dsp.k = lowpass(hz, self.rate as f32);
            }
            // a volume change takes about 20 ms
            let step = 1.0 / (0.02 * self.rate as f32);
            let mut buf = v.buf.lock().unwrap();
            let frames = (out.len() / 2).min(buf.len() / 2);
            for f in 0..frames {
                dsp.gain += (target - dsp.gain).clamp(-step, step);
                for c in 0..2 {
                    let x = buf.pop_front().unwrap_or(0.0);
                    let y = if dsp.hz > 0.0 {
                        let [b0, b1, b2, a1, a2] = dsp.k;
                        let s = &mut dsp.s[c];
                        let y = b0 * x + b1 * s[0] + b2 * s[1] - a1 * s[2] - a2 * s[3];
                        *s = [x, s[0], y, s[2]];
                        y
                    } else {
                        x
                    };
                    out[f * 2 + c] += y * dsp.gain;
                }
            }
            drop(buf);
            v.played.fetch_add(frames as u64, Ordering::Relaxed);
        }
    }
}

/// A second-order low-pass (RBJ's cookbook, Q = 1/sqrt(2)): (b0, b1, b2, a1, a2), a0 = 1.
fn lowpass(hz: f32, rate: f32) -> [f32; 5] {
    if hz <= 0.0 {
        return [1.0, 0.0, 0.0, 0.0, 0.0];
    }
    let w = 2.0 * std::f32::consts::PI * hz.min(rate * 0.45) / rate;
    let alpha = w.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let cos = w.cos();
    let a0 = 1.0 + alpha;
    let b1 = (1.0 - cos) / a0;
    [b1 / 2.0, b1, b1 / 2.0, -2.0 * cos / a0, (1.0 - alpha) / a0]
}

// ---- a stream -------------------------------------------------------------------------

/// What the guest reads about a stream (the threads write it).
#[derive(Default)]
struct Info {
    state: u32,
    error: String,
    caps: u32,
    duration: f64,
    name: Option<String>,
    title: Option<String>,
    codec: Option<String>,
    bitrate: Option<u32>,
    version: u32,
}

impl Info {
    fn metadata(&self) -> String {
        serde_json::json!({ "name": self.name, "title": self.title, "codec": self.codec, "bitrate": self.bitrate }).to_string()
    }
}

struct Stream {
    voice: Arc<Voice>,
    info: Arc<Mutex<Info>>,
    /// the guest asked to play (the decoder starts on the first play)
    want: Arc<AtomicBool>,
    started: bool,
    closed: Arc<AtomicBool>,
    url: String,
    /// when it was last paused (a live stream drops its connection after a while)
    paused_at: Arc<Mutex<Option<Instant>>>,
}

/// The game's streams.
pub struct Media {
    policy: MediaPolicy,
    bus: Option<Arc<MediaBus>>,
    /// None: closed (it reads as FAILED)
    streams: BTreeMap<i32, Option<Stream>>,
    next: i32,
    last_refusal: String,
    /// the bus the output drains, when the host has one (else a silent one, made on first use)
    output: Option<Arc<MediaBus>>,
}

impl Default for Media {
    fn default() -> Media {
        Media::new(MediaPolicy::default())
    }
}

impl Media {
    pub fn new(policy: MediaPolicy) -> Media {
        Media { policy, bus: None, streams: BTreeMap::new(), next: 1, last_refusal: String::new(), output: None }
    }

    /// The bus the window runner's audio output drains (before the game runs).
    pub fn set_output(&mut self, bus: Option<Arc<MediaBus>>) {
        self.output = bus;
    }

    fn bus(&mut self) -> Arc<MediaBus> {
        if self.bus.is_none() {
            self.bus = Some(self.output.clone().unwrap_or_else(MediaBus::silent));
        }
        self.bus.clone().expect("bus")
    }

    pub fn open(&mut self, desc: &str) -> i32 {
        let refuse = |m: &mut Media, why: String| {
            eprintln!("[gasm] media: refused: {why}");
            m.last_refusal = why;
            -1
        };
        let url = match parse_desc(desc) {
            Ok(u) => u,
            Err(e) => return refuse(self, e),
        };
        if let Err(why) = self.policy.may_open() {
            return refuse(self, why);
        }
        if self.streams.values().filter(|s| s.is_some()).count() >= MAX_STREAMS {
            return refuse(self, format!("{MAX_STREAMS} streams are open already"));
        }
        let h = self.next;
        self.next += 1;
        let voice = Arc::new(Voice::new());
        self.bus().add(voice.clone());
        let info = Arc::new(Mutex::new(Info { duration: -1.0, ..Default::default() }));
        self.streams.insert(
            h,
            Some(Stream { voice, info, want: Arc::new(AtomicBool::new(false)), started: false, closed: Arc::new(AtomicBool::new(false)), url, paused_at: Arc::new(Mutex::new(None)) }),
        );
        h
    }

    fn get(&mut self, h: i32) -> Result<Option<&mut Stream>, String> {
        match self.streams.get_mut(&h) {
            Some(s) => Ok(s.as_mut()),
            None => Err(format!("gasm:media: {h} is not a stream handle")),
        }
    }

    pub fn play(&mut self, h: i32) -> Result<(), String> {
        let bus = self.bus();
        let policy = self.policy.clone();
        let Some(s) = self.get(h)? else { return Ok(()) };
        s.want.store(true, Ordering::Release);
        *s.paused_at.lock().unwrap() = None;
        if !s.started {
            s.started = true;
            let (url, voice, info, want, closed, paused_at) = (s.url.clone(), s.voice.clone(), s.info.clone(), s.want.clone(), s.closed.clone(), s.paused_at.clone());
            let ctl = Control { voice, info, want, closed, paused_at, rate: bus.rate };
            std::thread::Builder::new()
                .name("gasm-media".into())
                .spawn(move || run(url, policy, ctl))
                .map_err(|e| format!("gasm:media: no thread: {e}"))?;
        }
        Ok(())
    }

    pub fn pause(&mut self, h: i32) -> Result<(), String> {
        let Some(s) = self.get(h)? else { return Ok(()) };
        if s.want.swap(false, Ordering::AcqRel) {
            s.voice.playing.store(false, Ordering::Release);
            *s.paused_at.lock().unwrap() = Some(Instant::now());
            let mut i = s.info.lock().unwrap();
            if matches!(i.state, PLAYING | CONNECTING) && s.started {
                i.state = PAUSED;
            }
        }
        Ok(())
    }

    pub fn close(&mut self, h: i32) -> Result<(), String> {
        let bus = self.bus.clone();
        match self.streams.get_mut(&h) {
            None => Err(format!("gasm:media: {h} is not a stream handle")),
            Some(slot) => {
                if let Some(s) = slot.take() {
                    s.closed.store(true, Ordering::Release);
                    if let Some(b) = bus {
                        b.remove(&s.voice);
                    }
                }
                Ok(())
            }
        }
    }

    pub fn set_volume(&mut self, h: i32, gain: f32) -> Result<(), String> {
        let g = if gain.is_nan() { 0.0 } else { gain.clamp(0.0, 1.0) };
        if let Some(s) = self.get(h)? {
            s.voice.gain.store(g.to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn set_lowpass(&mut self, h: i32, hz: f32) -> Result<(), String> {
        let hz = if hz.is_nan() || hz <= 0.0 { 0.0 } else { hz.clamp(20.0, 20_000.0) };
        if let Some(s) = self.get(h)? {
            s.voice.lowpass.store(hz.to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    fn info<R>(&mut self, h: i32, f: impl FnOnce(&Info) -> R, closed: R) -> Result<R, String> {
        Ok(match self.get(h)? {
            Some(s) => f(&s.info.lock().unwrap()),
            None => closed,
        })
    }

    pub fn caps(&mut self, h: i32) -> Result<u32, String> {
        self.info(h, |i| i.caps, 0)
    }

    pub fn state(&mut self, h: i32) -> Result<u32, String> {
        self.info(h, |i| i.state, FAILED)
    }

    pub fn error(&mut self, h: i32) -> Result<String, String> {
        if h == -1 {
            return Ok(self.last_refusal.clone());
        }
        self.info(h, |i| i.error.clone(), "closed".into())
    }

    pub fn position(&mut self, h: i32) -> Result<f64, String> {
        let rate = self.bus.as_ref().map_or(48_000, |b| b.rate) as f64;
        Ok(match self.get(h)? {
            Some(s) => s.voice.played.load(Ordering::Relaxed) as f64 / rate,
            None => 0.0,
        })
    }

    pub fn duration(&mut self, h: i32) -> Result<f64, String> {
        self.info(h, |i| i.duration, -1.0)
    }

    pub fn metadata(&mut self, h: i32) -> Result<String, String> {
        self.info(h, Info::metadata, Info::default().metadata())
    }

    pub fn metadata_version(&mut self, h: i32) -> Result<u32, String> {
        self.info(h, |i| i.version, 0)
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        for s in self.streams.values().flatten() {
            s.closed.store(true, Ordering::Release);
            if let Some(b) = &self.bus {
                b.remove(&s.voice);
            }
        }
    }
}

// ---- the threads ----------------------------------------------------------------------

struct Control {
    voice: Arc<Voice>,
    info: Arc<Mutex<Info>>,
    want: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    paused_at: Arc<Mutex<Option<Instant>>>,
    rate: u32,
}

impl Control {
    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn set(&self, f: impl FnOnce(&mut Info)) {
        f(&mut self.info.lock().unwrap());
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let roots = rustls_native_certs::load_native_certs();
        let certs = roots.certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::Specific(Arc::new(certs)))
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        // no overall timeout: a live stream never ends (a stall is found by the decoder)
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .tls_config(tls)
            .build()
            .into()
    })
}

/// What a connection learned from the response's headers.
#[derive(Default)]
struct Head {
    metaint: Option<usize>,
    name: Option<String>,
    bitrate: Option<u32>,
    content_type: Option<String>,
    /// no length and ICY or chunked: a live stream (reconnects; no duration)
    live: bool,
}

/// GET `url` (following redirects, each checked): the body's chunks go to `tx`, ICY
/// titles to `info`. Returns the head first through `head_tx`.
fn connect(url: &str, policy: &MediaPolicy, ctl: &Control) -> Result<(Head, ureq::Body), Refused> {
    let mut url = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        policy.check_url(&url)?;
        let req = ureq::http::Request::builder()
            .method("GET")
            .uri(url.as_str())
            .header("user-agent", crate::fetch::user_agent(None))
            .header("icy-metadata", "1")
            .body(())
            .map_err(|e| format!("{url}: {e}"))?;
        let resp = agent().run(req).map_err(|e| Refused::Retry(format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        let header = |n: &str| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_owned());
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let Some(loc) = header("location") else { return Err(format!("{url}: a redirect without a location").into()) };
            url = crate::fetch::resolve_url(&url, &loc);
            if ctl.closed() {
                return Err(Refused::Fatal(String::new()));
            }
            continue;
        }
        if status >= 500 {
            return Err(Refused::Retry(format!("{url}: HTTP {status}")));
        }
        if status != 200 {
            return Err(Refused::Fatal(format!("{url}: HTTP {status}")));
        }
        let metaint = header("icy-metaint").and_then(|v| v.parse().ok()).filter(|&n: &usize| n > 0 && n <= 1 << 20);
        let length = header("content-length").is_some();
        let head = Head {
            metaint,
            name: header("icy-name").filter(|n| !n.is_empty()).map(|n| n.chars().take(MAX_TITLE).collect()),
            bitrate: header("icy-br").and_then(|v| v.split(',').next().and_then(|b| b.trim().parse().ok())),
            content_type: header("content-type").map(|c| c.to_ascii_lowercase()),
            live: metaint.is_some() || header("icy-name").is_some() || !length,
        };
        return Ok((head, resp.into_body()));
    }
    Err(Refused::Fatal(format!("{url}: too many redirects")))
}

/// The connection's thread: reads the body, cuts the ICY blocks out (their titles go to
/// `info`) and sends the audio bytes on. Ends at the body's end or when nobody listens.
fn pump(body: ureq::Body, head_metaint: Option<usize>, info: Arc<Mutex<Info>>, tx: SyncSender<Vec<u8>>, closed: Arc<AtomicBool>) {
    let mut reader = body.into_reader();
    let mut icy = head_metaint.map(Icy::new);
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        if closed.load(Ordering::Acquire) {
            return;
        }
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let audio = match &mut icy {
            Some(icy) => {
                let (audio, titles) = icy.feed(&buf[..n]);
                for t in titles {
                    let mut i = info.lock().unwrap();
                    if i.title.as_deref() != Some(&t) {
                        i.title = Some(t);
                        i.version = i.version.wrapping_add(1);
                    }
                }
                audio
            }
            None => buf[..n].to_vec(),
        };
        if !audio.is_empty() && tx.send(audio).is_err() {
            return;
        }
    }
}

/// ICY's in-band metadata: after every `metaint` bytes of audio, a length byte (times 16)
/// and that much text (`StreamTitle='...';`).
struct Icy {
    metaint: usize,
    /// audio bytes left before the next block
    left: usize,
    /// metadata bytes left in the current block, and the block so far
    meta_left: Option<usize>,
    meta: Vec<u8>,
}

impl Icy {
    fn new(metaint: usize) -> Icy {
        Icy { metaint, left: metaint, meta_left: None, meta: Vec::new() }
    }

    /// The audio in `data`, and the titles of the blocks that ended in it.
    fn feed(&mut self, mut data: &[u8]) -> (Vec<u8>, Vec<String>) {
        let (mut audio, mut titles) = (Vec::with_capacity(data.len()), Vec::new());
        while !data.is_empty() {
            match self.meta_left {
                None if self.left > 0 => {
                    let n = self.left.min(data.len());
                    audio.extend_from_slice(&data[..n]);
                    self.left -= n;
                    data = &data[n..];
                }
                None => {
                    self.meta_left = Some(data[0] as usize * 16);
                    self.meta.clear();
                    data = &data[1..];
                }
                Some(m) => {
                    let n = m.min(data.len());
                    self.meta.extend_from_slice(&data[..n]);
                    data = &data[n..];
                    if m == n {
                        if let Some(t) = stream_title(&self.meta) {
                            titles.push(t);
                        }
                        self.meta_left = None;
                        self.left = self.metaint;
                    } else {
                        self.meta_left = Some(m - n);
                    }
                }
            }
            if self.meta_left == Some(0) {
                self.meta_left = None;
                self.left = self.metaint;
            }
        }
        (audio, titles)
    }
}

/// `StreamTitle='Artist - Song';StreamUrl='...';` -> the title (UTF-8, else Latin-1).
pub fn stream_title(meta: &[u8]) -> Option<String> {
    let end = meta.iter().position(|&b| b == 0).unwrap_or(meta.len());
    let meta = &meta[..end];
    let key = b"StreamTitle='";
    let start = meta.windows(key.len()).position(|w| w == key)? + key.len();
    let rest = &meta[start..];
    let stop = rest.windows(2).position(|w| w == b"';").unwrap_or(rest.len());
    let raw = &rest[..stop];
    let text = match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| b as char).collect(),
    };
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut t = text.to_owned();
    while t.len() > MAX_TITLE {
        t.pop();
    }
    Some(t)
}

/// The decoder's input: the connection's chunks, as a reader (symphonia's source must
/// be `Sync`, a network body isn't). No data for STALL: an error, so the stream reconnects.
struct Pipe {
    rx: Mutex<Receiver<Vec<u8>>>,
    chunk: Vec<u8>,
    at: usize,
    closed: Arc<AtomicBool>,
}

impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.at >= self.chunk.len() {
            let deadline = Instant::now() + STALL;
            loop {
                if self.closed.load(Ordering::Acquire) {
                    return Ok(0);
                }
                match self.rx.lock().unwrap().recv_timeout(Duration::from_millis(250)) {
                    Ok(c) => {
                        self.chunk = c;
                        self.at = 0;
                        break;
                    }
                    Err(RecvTimeoutError::Disconnected) => return Ok(0),
                    Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the stream stalled"));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
        let n = out.len().min(self.chunk.len() - self.at);
        out[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Why a connection couldn't start: worth another try (the network, a 5xx), or not
/// (refused, not found, not audio).
enum Refused {
    Retry(String),
    Fatal(String),
}

impl From<String> for Refused {
    fn from(why: String) -> Refused {
        Refused::Fatal(why)
    }
}

/// How one connection ended.
enum End {
    /// the body ended (a track: drain and end; a live stream: reconnect)
    Eof,
    /// the connection broke or stalled (a live stream reconnects)
    Dropped(String),
    /// paused long enough to let go of a live stream (reconnect on play)
    Released,
    /// closed by the guest
    Closed,
}

/// The stream's thread: connect (waiting for the player's answer first), decode, refill
/// the buffer, reconnect a live stream that drops, and report state and metadata.
fn run(url: String, policy: MediaPolicy, ctl: Control) {
    // the player's answer (asked up front from the manifest; pending meanwhile)
    loop {
        if ctl.closed() {
            return;
        }
        match policy.answered() {
            Ok(true) => break,
            Ok(false) => std::thread::sleep(Duration::from_millis(50)),
            Err(why) => return fail(&ctl, why),
        }
    }
    let mut tries = 0;
    loop {
        if ctl.closed() {
            return;
        }
        // paused before (re)connecting: wait for play
        while !ctl.want.load(Ordering::Acquire) {
            if ctl.closed() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        ctl.set(|i| {
            if i.state != PAUSED {
                i.state = CONNECTING;
            }
        });
        let began = Instant::now();
        let outcome = connection(&url, &policy, &ctl);
        // a connection that lasted counts as good: the next drop starts the tries anew
        if began.elapsed() >= Duration::from_secs(10) {
            tries = 0;
        }
        let live = match outcome {
            Ok((End::Closed, _)) => return,
            Ok((End::Eof, false)) => return finish(&ctl),
            Ok((End::Released, _)) => {
                tries = 0;
                continue;
            }
            Ok((End::Eof | End::Dropped(_), true)) => true,
            Ok((End::Dropped(why), false)) => return fail(&ctl, why),
            Err(Refused::Fatal(why)) if why.is_empty() => return,
            Err(Refused::Fatal(why)) => return fail(&ctl, why),
            Err(Refused::Retry(why)) => {
                if tries >= RECONNECTS {
                    return fail(&ctl, why);
                }
                eprintln!("[gasm] media: {why}");
                false
            }
        };
        if live {
            eprintln!("[gasm] media: {url}: the stream dropped, reconnecting");
            // the game sees it at once (what's buffered still plays meanwhile)
            ctl.set(|i| {
                if i.state != PAUSED {
                    i.state = CONNECTING;
                }
            });
        }
        tries += 1;
        if tries > RECONNECTS {
            return fail(&ctl, format!("{url}: the stream dropped {RECONNECTS} times"));
        }
        std::thread::sleep(Duration::from_secs(1 << (tries - 1)));
    }
}

fn fail(ctl: &Control, why: String) {
    eprintln!("[gasm] media: {why}");
    ctl.voice.playing.store(false, Ordering::Release);
    ctl.set(|i| {
        i.state = FAILED;
        i.error = why;
    });
}

/// A track's end: let what's buffered play out, then ENDED.
fn finish(ctl: &Control) {
    while ctl.voice.buffered() > 0 && !ctl.closed() {
        std::thread::sleep(Duration::from_millis(20));
    }
    ctl.voice.playing.store(false, Ordering::Release);
    ctl.set(|i| i.state = ENDED);
}

/// One connection: Ok((how it ended, whether the stream is live)), or Err(why) if it
/// couldn't start.
fn connection(url: &str, policy: &MediaPolicy, ctl: &Control) -> Result<(End, bool), Refused> {
    use symphonia::core::codecs::CodecParameters;
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
    use symphonia::core::meta::MetadataOptions;

    let (head, body) = connect(url, policy, ctl)?;
    let live = head.live;
    ctl.set(|i| {
        let (name, bitrate) = (head.name.clone().or(i.name.clone()), head.bitrate.or(i.bitrate));
        if (&name, bitrate) != (&i.name, i.bitrate) {
            (i.name, i.bitrate) = (name, bitrate);
            i.version = i.version.wrapping_add(1);
        }
    });
    let (tx, rx) = sync_channel::<Vec<u8>>(64);
    {
        let (info, closed) = (ctl.info.clone(), ctl.closed.clone());
        let metaint = head.metaint;
        std::thread::Builder::new()
            .name("gasm-media-net".into())
            .spawn(move || pump(body, metaint, info, tx, closed))
            .map_err(|e| format!("no thread: {e}"))?;
    }
    let pipe = Pipe { rx: Mutex::new(rx), chunk: Vec::new(), at: 0, closed: ctl.closed.clone() };
    let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(pipe)), Default::default());
    let mut hint = Hint::new();
    match head.content_type.as_deref().map(|c| c.split(';').next().unwrap_or("").trim()) {
        Some("audio/mpeg" | "audio/mp3" | "audio/mpeg3") => drop(hint.with_extension("mp3")),
        Some("audio/aac" | "audio/aacp" | "audio/x-aac") => drop(hint.with_extension("aac")),
        Some("audio/ogg" | "application/ogg" | "audio/vorbis") => drop(hint.with_extension("ogg")),
        Some("audio/flac" | "audio/x-flac") => drop(hint.with_extension("flac")),
        _ => {}
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("{url}: not a stream gasm can play ({e})"))?;
    let track = format.default_track(TrackType::Audio).ok_or_else(|| format!("{url}: no audio in it"))?;
    let id = track.id;
    let Some(CodecParameters::Audio(params)) = &track.codec_params else { return Err(format!("{url}: no audio in it").into()) };
    let codec = symphonia::default::get_codecs()
        .get_audio_decoder(params.codec)
        .map(|c| c.codec.info.short_name.to_string())
        .unwrap_or_else(|| "?".into());
    let duration = match (track.num_frames, params.sample_rate) {
        (Some(n), Some(r)) if !live && r > 0 => n as f64 / r as f64,
        _ => -1.0,
    };
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| format!("{url}: can't decode {codec} ({e})"))?;
    ctl.set(|i| {
        if i.codec.as_deref() != Some(codec.as_str()) {
            i.codec = Some(codec.clone());
            i.version = i.version.wrapping_add(1);
        }
        i.duration = duration;
        i.caps = CAN_VOLUME | CAN_LOWPASS | CAN_METADATA;
    });

    let mut resampler = Resampler::new(ctl.rate);
    let (mut src_rate, mut src_ch) = (0u32, 0usize);
    let (mut samples, mut bytes, mut out) = (Vec::<f32>::new(), Vec::<u8>::new(), Vec::<f32>::new());
    let cap = (BUFFER_S * ctl.rate as f64) as usize;
    let start = (START_S * ctl.rate as f64) as usize;
    // this connection's audio, in frames at the output's rate: PLAYING once it has START_S
    // (after a reconnect the output may still be playing what the last one left)
    let (mut announced, mut decoded) = (false, 0usize);
    loop {
        if ctl.closed() {
            return Ok((End::Closed, live));
        }
        // paused: hold the buffer; a live stream lets go after a while
        if !ctl.want.load(Ordering::Acquire) {
            let since = *ctl.paused_at.lock().unwrap();
            if live && since.is_some_and(|t| t.elapsed() >= PAUSE_DROP) {
                ctl.voice.buf.lock().unwrap().clear();
                return Ok((End::Released, live));
            }
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        // playing: the output takes it once enough is buffered (each connection says so)
        let resumed = announced && !ctl.voice.playing.load(Ordering::Acquire) && ctl.voice.buffered() >= start;
        if (!announced && decoded >= start) || resumed {
            announced = true;
            ctl.voice.playing.store(true, Ordering::Release);
            ctl.set(|i| {
                i.state = PLAYING;
                i.error.clear();
            });
        }
        if ctl.voice.buffered() >= cap {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => return Ok((End::Eof, live)),
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok((End::Eof, live)),
            Err(e) => return Ok((End::Dropped(format!("{url}: {e}")), live)),
        };
        if packet.track_id != id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            // a damaged frame: skip it, as players do
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Ok((End::Dropped(format!("{url}: {e}")), live)),
        };
        let (rate, ch) = (buf.spec().rate(), buf.spec().channels().count());
        if ch == 0 || rate == 0 {
            continue;
        }
        if (rate, ch) != (src_rate, src_ch) {
            (src_rate, src_ch) = (rate, ch);
            resampler.configure(rate, ch.min(2) as u32);
        }
        samples.clear();
        buf.copy_to_vec_interleaved(&mut samples);
        // the resampler takes mono or stereo: more channels keep the first two
        bytes.clear();
        for frame in samples.chunks(ch) {
            for s in &frame[..ch.min(2)] {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
        }
        out.clear();
        resampler.process(&bytes, &mut out);
        decoded += out.len() / 2;
        ctl.voice.buf.lock().unwrap().extend(out.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions() {
        assert_eq!(parse_desc(r#"{"url":"https://radio.example/stream"}"#).unwrap(), "https://radio.example/stream");
        for bad in [r#"[]"#, r#"{}"#, r#"{"url":"http://radio.example/"}"#, r#"{"url":"https://"}"#, r#"{"url":"https://a b/"}"#] {
            assert!(parse_desc(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn addresses() {
        for ip in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.0.1", "100.64.0.1", "0.0.0.0", "::1", "fc00::1", "fe80::1", "::ffff:192.168.0.1", "224.0.0.1"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
        let p = MediaPolicy::new(Some(vec![]));
        assert!(p.check_url("https://127.0.0.1:8443/x").is_err());
        assert!(p.check_url("https://[::1]/x").is_err());
        let listed = MediaPolicy::new(Some(vec!["127.0.0.1".into()]));
        assert!(listed.check_url("https://127.0.0.1:8443/x").is_ok());
        assert!(listed.check_url("https://1.1.1.1/x").is_err(), "a list allows only its hosts");
        assert!(MediaPolicy::new(Some(vec![])).check_url("http://1.1.1.1/x").is_err());
    }

    #[test]
    fn permission() {
        assert!(MediaPolicy::default().may_open().is_err(), "no manifest entry, no flag");
        let declared = MediaPolicy { declared: true, ..Default::default() };
        assert!(declared.may_open().is_err(), "headless: no one to ask");
        assert!(MediaPolicy::new(Some(vec![])).may_open().is_ok());
    }

    #[test]
    fn icy_blocks() {
        let mut icy = Icy::new(4);
        let meta = b"StreamTitle='Band - Song';StreamUrl='';";
        let blocks = meta.len().div_ceil(16);
        let mut data = b"abcd".to_vec();
        data.push(blocks as u8);
        data.extend_from_slice(meta);
        data.resize(5 + blocks * 16, 0);
        data.extend_from_slice(b"efgh");
        data.push(0); // an empty block
        data.extend_from_slice(b"ij");
        // fed in odd pieces: the same result
        let (mut audio, mut titles) = (Vec::new(), Vec::new());
        for piece in data.chunks(3) {
            let (a, t) = icy.feed(piece);
            audio.extend(a);
            titles.extend(t);
        }
        assert_eq!(audio, b"abcdefghij");
        assert_eq!(titles, vec!["Band - Song".to_string()]);
        assert_eq!(stream_title(b"StreamTitle='Caf\xe9';"), Some("Café".into()));
        assert_eq!(stream_title(b"StreamTitle='';"), None);
    }

    #[test]
    fn filter_is_unity_at_dc() {
        let [b0, b1, b2, a1, a2] = lowpass(1000.0, 48_000.0);
        assert!(((b0 + b1 + b2) / (1.0 + a1 + a2) - 1.0).abs() < 1e-4);
    }
}
