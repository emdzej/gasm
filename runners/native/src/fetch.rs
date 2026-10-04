//! gasm:fetch for the native runner: HTTP(S) requests made for the guest
//! (design/fetch.md).
//!
//! Each request runs on a background thread (ureq, rustls with ring, the OS
//! certificate store or `SSL_CERT_FILE`); the guest polls a handle, so nothing
//! blocks a frame. Redirects are followed here, not by ureq, so every hop is
//! checked against the host list. The body arrives in chunks through a bounded
//! channel: a guest that stops reading pauses the download.
//!
//! Headless runs can record responses (`--fetch-record DIR`) and replay them
//! (`--fetch-replay DIR`): a replayed request completes at the start of the
//! next frame, on every runner, so hashes compare. The record format and key
//! are shared with runners/web/lib/fetch.js.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::net::{url_host, NetPolicy};

pub const PENDING: u32 = 0;
pub const HEADERS: u32 = 1;
pub const DONE: u32 = 2;
pub const FAILED: u32 = 3;

pub const MAX_REQUESTS: usize = 16;
/// Largest response body; a larger one fails the request.
pub const MAX_BODY: u64 = 64 << 20;
pub const MAX_REQUEST_BODY: usize = 16 << 20;
const MAX_REDIRECTS: usize = 10;
const CHUNK: usize = 64 << 10;
/// Chunks in flight per request (4 MiB): then the thread waits for the guest.
const QUEUE: usize = 64;

pub const METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
/// Request headers browsers don't let pages set (the Fetch standard's forbidden
/// request headers, plus user-agent): a request with one fails on every runner,
/// so a game can't come to depend on it. Same list in runners/web/lib/fetch.js.
pub const FORBIDDEN_HEADERS: [&str; 22] = [
    "accept-charset", "accept-encoding", "access-control-request-headers", "access-control-request-method",
    "connection", "content-length", "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "set-cookie", "te", "trailer", "transfer-encoding", "upgrade", "user-agent", "via",
];
pub const FORBIDDEN_PREFIXES: [&str; 2] = ["proxy-", "sec-"];
/// Response headers no runner reports: connection-level ones (browsers' fetch() hides
/// them or they differ by HTTP version) and cookies (fetch() never exposes them).
/// Same list in runners/web/lib/fetch.js.
pub const HIDDEN_RESPONSE_HEADERS: [&str; 9] =
    ["connection", "keep-alive", "proxy-connection", "set-cookie", "set-cookie2", "te", "trailer", "transfer-encoding", "upgrade"];

/// Response headers as the guest sees them, the way fetch()'s Headers lists them:
/// `name: value` lines sorted by (lowercase) name, repeated names joined with ", ".
/// The body the guest reads is always decoded (gzip natively; browsers decode
/// everything), so `content-encoding` is left out, and `content-length` too when it
/// counted encoded bytes.
pub fn header_lines<'a>(headers: impl IntoIterator<Item = (&'a str, String)>) -> String {
    let mut by_name: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (k, v) in headers {
        let k = k.to_ascii_lowercase();
        if !HIDDEN_RESPONSE_HEADERS.contains(&k.as_str()) {
            by_name.entry(k).or_default().push(v.trim().to_owned());
        }
    }
    if let Some(enc) = by_name.remove("content-encoding") {
        if enc.iter().any(|e| !e.eq_ignore_ascii_case("identity")) {
            by_name.remove("content-length");
        }
    }
    by_name.into_iter().map(|(k, v)| format!("{k}: {}\n", v.join(", "))).collect()
}

/// Where responses come from.
#[derive(Clone, Debug, Default)]
pub enum FetchMode {
    #[default]
    Live,
    /// live, and every completed response is stored in the directory
    Record(PathBuf),
    /// never the network: responses from the directory, complete at the next frame
    Replay(PathBuf),
}

/// A request the guest described (validated).
#[derive(Debug, PartialEq)]
pub struct Desc {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// Parse and check a request description; Err says why it's refused.
pub fn parse_desc(json: &str, body_len: usize) -> Result<Desc, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let o = v.as_object().ok_or("the description must be a JSON object")?;
    let method = match o.get("method") {
        None => "GET".to_owned(),
        Some(m) => m.as_str().ok_or("method must be a string")?.to_ascii_uppercase(),
    };
    if !METHODS.contains(&method.as_str()) {
        return Err(format!("method {method} is not supported"));
    }
    let url = o.get("url").and_then(|u| u.as_str()).ok_or("url must be a string")?.to_owned();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) || url_host(&url).is_none() {
        return Err(format!("not an absolute http(s) URL: {url}"));
    }
    if url.chars().any(|c| c.is_ascii_control() || c == ' ') {
        return Err(format!("invalid URL: {url}"));
    }
    let mut headers = Vec::new();
    if let Some(h) = o.get("headers") {
        for (k, v) in h.as_object().ok_or("headers must be an object")? {
            let v = v.as_str().ok_or_else(|| format!("header {k}: the value must be a string"))?;
            check_header(k, v)?;
            headers.push((k.to_ascii_lowercase(), v.to_owned()));
        }
    }
    if body_len > 0 && (method == "GET" || method == "HEAD") {
        return Err(format!("a {method} request can't have a body"));
    }
    if body_len > MAX_REQUEST_BODY {
        return Err(format!("request body over {MAX_REQUEST_BODY} bytes"));
    }
    Ok(Desc { method, url, headers })
}

fn check_header(name: &str, value: &str) -> Result<(), String> {
    let token = |c: u8| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c);
    if name.is_empty() || !name.bytes().all(token) {
        return Err(format!("invalid header name {name:?}"));
    }
    if value.bytes().any(|c| c == b'\r' || c == b'\n' || c == 0) {
        return Err(format!("invalid value for header {name}"));
    }
    let n = name.to_ascii_lowercase();
    if FORBIDDEN_HEADERS.contains(&n.as_str()) || FORBIDDEN_PREFIXES.iter().any(|p| n.starts_with(p)) {
        return Err(format!("header {name} can't be set (browsers refuse it)"));
    }
    Ok(())
}

/// FNV-1a 64 of method, URL and body: the name of a recorded response.
pub fn record_key(method: &str, url: &str, body: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in method.bytes().chain([0]).chain(url.bytes()).chain([0]).chain(body.iter().copied()) {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

enum Event {
    Head(i32, String),
    Chunk(Vec<u8>),
    Done,
    Fail(String),
}

struct Req {
    events: Option<Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    state: u32,
    status: i32,
    headers: Option<String>,
    body: VecDeque<u8>,
    /// replayed: nothing is visible before this frame
    ready_frame: u64,
}

pub struct Fetch {
    policy: NetPolicy,
    mode: FetchMode,
    reqs: HashMap<i32, Req>,
    next: i32,
    /// hosts already logged as denied (once each)
    denied: std::collections::HashSet<String>,
}

impl Default for Fetch {
    fn default() -> Fetch {
        Fetch::new(NetPolicy::default(), FetchMode::Live)
    }
}

impl Fetch {
    pub fn new(policy: NetPolicy, mode: FetchMode) -> Fetch {
        Fetch { policy, mode, reqs: HashMap::new(), next: 1, denied: Default::default() }
    }

    /// Start a request; a handle > 0 or -1 (logged). `frame` is the current frame index.
    pub fn request(&mut self, desc: &str, body: Vec<u8>, frame: u64) -> i32 {
        let d = match parse_desc(desc, body.len()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[gasm] fetch: refused: {e}");
                return -1;
            }
        };
        if self.reqs.len() >= MAX_REQUESTS {
            eprintln!("[gasm] fetch: too many requests (max {MAX_REQUESTS}): {}", d.url);
            return -1;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let mut req = Req { events: None, cancel: cancel.clone(), state: PENDING, status: 0, headers: None, body: VecDeque::new(), ready_frame: 0 };
        match &self.mode {
            FetchMode::Replay(dir) => {
                req.ready_frame = frame + 1;
                match load_record(dir, &record_key(&d.method, &d.url, &body)) {
                    Some((status, headers, data)) => {
                        (req.state, req.status, req.headers, req.body) = (DONE, status, Some(headers), data.into());
                    }
                    None => {
                        eprintln!("[gasm] fetch: {} {} is not recorded in {}", d.method, d.url, dir.display());
                        req.state = FAILED;
                    }
                }
            }
            mode => {
                if let Some(why) = self.policy.refusal(&d.url) {
                    let host = url_host(&d.url).unwrap_or_default();
                    if self.denied.insert(host) {
                        eprintln!("[gasm] fetch: denied {} ({why})", d.url);
                    }
                    return -1;
                }
                let record = match mode {
                    FetchMode::Record(dir) => Some(dir.join(record_key(&d.method, &d.url, &body))),
                    _ => None,
                };
                let (tx, rx) = mpsc::sync_channel(QUEUE);
                let policy = self.policy.clone();
                std::thread::spawn(move || run(d, body, policy, tx, cancel, record));
                req.events = Some(rx);
            }
        }
        let h = self.next;
        self.next += 1;
        self.reqs.insert(h, req);
        h
    }

    /// A handle `request` never returned is an error (the guest traps); a closed one is None.
    fn get(&mut self, h: i32) -> Result<Option<&mut Req>, String> {
        if h <= 0 || h >= self.next {
            return Err(format!("gasm:fetch: invalid handle {h}"));
        }
        let Some(r) = self.reqs.get_mut(&h) else { return Ok(None) };
        if let Some(rx) = &r.events {
            loop {
                match rx.try_recv() {
                    Ok(Event::Head(status, headers)) => (r.state, r.status, r.headers) = (HEADERS, status, Some(headers)),
                    Ok(Event::Chunk(c)) => r.body.extend(c),
                    Ok(Event::Done) => r.state = DONE,
                    Ok(Event::Fail(e)) => {
                        eprintln!("[gasm] fetch: {e}");
                        (r.state, r.status) = (FAILED, 0);
                        r.body.clear();
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if r.state < DONE {
                            (r.state, r.status) = (FAILED, 0);
                        }
                        break;
                    }
                }
            }
            if r.state >= DONE {
                r.events = None;
            }
        }
        Ok(Some(r))
    }

    /// (state, status, headers) as the guest may see them now.
    fn view(&mut self, h: i32, frame: u64) -> Result<(u32, i32, Option<String>), String> {
        Ok(match self.get(h)? {
            None => (FAILED, 0, None),
            Some(r) if frame < r.ready_frame => (PENDING, 0, None),
            Some(r) => (r.state, r.status, r.headers.clone()),
        })
    }

    pub fn state(&mut self, h: i32, frame: u64) -> Result<u32, String> {
        Ok(self.view(h, frame)?.0)
    }

    pub fn status(&mut self, h: i32, frame: u64) -> Result<i32, String> {
        Ok(self.view(h, frame)?.1)
    }

    pub fn headers(&mut self, h: i32, frame: u64) -> Result<Option<String>, String> {
        Ok(self.view(h, frame)?.2)
    }

    /// Up to `cap` body bytes: Ok(bytes) (empty: none yet), or Err(()) when done and drained or failed.
    pub fn read(&mut self, h: i32, cap: usize, frame: u64) -> Result<Result<Vec<u8>, ()>, String> {
        let Some(r) = self.get(h)? else { return Ok(Err(())) };
        if frame < r.ready_frame || r.state == PENDING {
            return Ok(Ok(Vec::new()));
        }
        if r.body.is_empty() {
            return Ok(if r.state >= DONE { Err(()) } else { Ok(Vec::new()) });
        }
        let n = cap.min(r.body.len());
        Ok(Ok(r.body.drain(..n).collect()))
    }

    pub fn close(&mut self, h: i32) -> Result<(), String> {
        if h <= 0 || h >= self.next {
            return Err(format!("gasm:fetch: invalid handle {h}"));
        }
        if let Some(r) = self.reqs.remove(&h) {
            r.cancel.store(true, Ordering::Relaxed); // the thread also stops when its channel closes
        }
        Ok(())
    }
}

fn load_record(dir: &Path, key: &str) -> Option<(i32, String, Vec<u8>)> {
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{key}.json"))).ok()?).ok()?;
    let body = std::fs::read(dir.join(format!("{key}.body"))).ok()?;
    Some((meta.get("status")?.as_i64()? as i32, meta.get("headers")?.as_str()?.to_owned(), body))
}

fn save_record(path: &Path, method: &str, url: &str, status: i32, headers: &str, body: &[u8]) {
    let meta = serde_json::json!({ "method": method, "url": url, "status": status, "headers": headers });
    let write = || -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path.with_extension("body"), body)?;
        std::fs::write(path.with_extension("json"), format!("{}\n", serde_json::to_string_pretty(&meta)?))
    };
    if let Err(e) = write() {
        eprintln!("[gasm] fetch: --fetch-record {}: {e}", path.display());
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        // the OS's trusted roots (or SSL_CERT_FILE), as for wss:// in gasm:net
        let roots = rustls_native_certs::load_native_certs();
        for e in &roots.errors {
            eprintln!("[gasm] fetch: certificates: {e}");
        }
        let certs = roots.certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::Specific(Arc::new(certs)))
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(120)))
            .tls_config(tls)
            .build()
            .into()
    })
}

/// `location` relative to `base` (absolute, origin-relative, or path-relative).
fn resolve(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_owned();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("http", base));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if let Some(l) = location.strip_prefix("//") {
        return format!("{scheme}://{l}");
    }
    if location.starts_with('/') {
        return format!("{scheme}://{authority}{location}");
    }
    let path = rest[authority.len()..].split(['?', '#']).next().unwrap_or("");
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    format!("{scheme}://{authority}{}{location}", if dir.is_empty() { "/" } else { dir })
}

fn run(d: Desc, body: Vec<u8>, policy: NetPolicy, tx: SyncSender<Event>, cancel: Arc<AtomicBool>, record: Option<PathBuf>) {
    let result = (|| -> Result<(), String> {
        let (mut method, mut url, mut body) = (d.method.clone(), d.url.clone(), body);
        let mut hops = 0;
        let resp = loop {
            let mut b = ureq::http::Request::builder().method(method.as_str()).uri(url.as_str());
            for (k, v) in &d.headers {
                b = b.header(k.as_str(), v.as_str());
            }
            let req = b.body(body.clone()).map_err(|e| format!("{url}: {e}"))?;
            let resp = agent().run(req).map_err(|e| format!("{method} {url}: {e}"))?;
            let status = resp.status().as_u16();
            let location = resp.headers().get("location").and_then(|v| v.to_str().ok()).map(str::to_owned);
            match (status, location) {
                (301 | 302 | 303 | 307 | 308, Some(loc)) => {
                    hops += 1;
                    if hops > MAX_REDIRECTS {
                        return Err(format!("{url}: too many redirects"));
                    }
                    let next = resolve(&url, &loc);
                    if let Some(why) = policy.refusal(&next) {
                        return Err(format!("{url}: redirect to {next} denied ({why})"));
                    }
                    // as browsers: 303, and 301/302 after a POST, continue as a GET without a body
                    if status == 303 || (matches!(status, 301 | 302) && method == "POST") {
                        if method != "HEAD" {
                            method = "GET".into();
                        }
                        body.clear();
                    }
                    url = next;
                }
                _ => break resp,
            }
        };
        let status = resp.status().as_u16() as i32;
        let headers = header_lines(resp.headers().iter().map(|(k, v)| (k.as_str(), String::from_utf8_lossy(v.as_bytes()).into_owned())));
        tx.send(Event::Head(status, headers.clone())).map_err(|_| String::new())?;
        let mut reader = resp.into_body().into_with_config().limit(MAX_BODY).reader();
        let mut buf = vec![0u8; CHUNK];
        let mut all = Vec::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                break;
            }
            if record.is_some() {
                all.extend_from_slice(&buf[..n]);
            }
            tx.send(Event::Chunk(buf[..n].to_vec())).map_err(|_| String::new())?;
        }
        if let Some(path) = &record {
            save_record(path, &d.method, &d.url, status, &headers, &all);
        }
        tx.send(Event::Done).map_err(|_| String::new())
    })();
    if let Err(e) = result {
        if !e.is_empty() {
            let _ = tx.send(Event::Fail(e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions() {
        let d = parse_desc(r#"{"url":"https://api.met.no/x?a=1","headers":{"Accept":"application/json"}}"#, 0).unwrap();
        assert_eq!(d, Desc { method: "GET".into(), url: "https://api.met.no/x?a=1".into(), headers: vec![("accept".into(), "application/json".into())] });
        assert_eq!(parse_desc(r#"{"method":"post","url":"http://h/"}"#, 3).unwrap().method, "POST");
        for bad in [
            r#"[]"#,
            r#"{"url":"ftp://h/"}"#,
            r#"{"url":"/relative"}"#,
            r#"{"method":"TRACE","url":"http://h/"}"#,
            r#"{"url":"http://h/","headers":{"Cookie":"a"}}"#,
            r#"{"url":"http://h/","headers":{"sec-fetch-mode":"a"}}"#,
            r#"{"url":"http://h/","headers":{"x":"a\r\nb"}}"#,
            r#"{"url":"http://h/","headers":{"bad name":"a"}}"#,
        ] {
            assert!(parse_desc(bad, 0).is_err(), "{bad}");
        }
        assert!(parse_desc(r#"{"url":"http://h/"}"#, 1).is_err()); // GET with a body
    }

    #[test]
    fn response_headers() {
        let h = header_lines([("Vary", "a".into()), ("content-type", "text/plain".into()), ("Connection", "close".into()), ("vary", " b ".into()), ("set-cookie", "x=1".into())]);
        assert_eq!(h, "content-type: text/plain\nvary: a, b\n");
        let gz = header_lines([("content-encoding", "gzip".into()), ("content-length", "10".into()), ("etag", "x".into())]);
        assert_eq!(gz, "etag: x\n");
    }

    #[test]
    fn redirects_resolve() {
        assert_eq!(resolve("https://a.b/x/y?q", "/z"), "https://a.b/z");
        assert_eq!(resolve("https://a.b/x/y?q", "z"), "https://a.b/x/z");
        assert_eq!(resolve("https://a.b", "z"), "https://a.b/z");
        assert_eq!(resolve("https://a.b/x", "//c.d/e"), "https://c.d/e");
        assert_eq!(resolve("https://a.b/x", "http://c.d/"), "http://c.d/");
    }

    #[test]
    fn keys() {
        // the same in runners/web/lib/fetch.js (recordKey)
        assert_eq!(record_key("GET", "https://example.org/", b""), "a0eecdfd36f8bde7");
    }

    #[test]
    fn replay_and_handles() {
        let dir = std::env::temp_dir().join(format!("gasm-fetch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let key = record_key("GET", "https://example.org/a", b"");
        save_record(&dir.join(&key), "GET", "https://example.org/a", 200, "content-type: text/plain\n", b"hello");
        let mut f = Fetch::new(NetPolicy::default(), FetchMode::Replay(dir.clone()));
        let h = f.request(r#"{"url":"https://example.org/a"}"#, vec![], 5);
        let miss = f.request(r#"{"url":"https://example.org/b"}"#, vec![], 5);
        assert_eq!((f.state(h, 5).unwrap(), f.status(h, 5).unwrap(), f.read(h, 9, 5).unwrap()), (PENDING, 0, Ok(vec![])));
        assert_eq!((f.state(h, 6).unwrap(), f.status(h, 6).unwrap()), (DONE, 200));
        assert_eq!(f.headers(h, 6).unwrap().as_deref(), Some("content-type: text/plain\n"));
        assert_eq!(f.read(h, 3, 6).unwrap(), Ok(b"hel".to_vec()));
        assert_eq!(f.read(h, 9, 6).unwrap(), Ok(b"lo".to_vec()));
        assert_eq!(f.read(h, 9, 6).unwrap(), Err(()));
        assert_eq!(f.state(miss, 6).unwrap(), FAILED);
        f.close(h).unwrap();
        assert_eq!(f.state(h, 7).unwrap(), FAILED); // closed
        assert!(f.state(99, 7).is_err()); // never returned: traps
        let _ = std::fs::remove_dir_all(&dir);
        // live requests are denied without --allow-net
        assert_eq!(Fetch::default().request(r#"{"url":"https://example.org/"}"#, vec![], 0), -1);
    }
}
