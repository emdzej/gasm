//! gasm:net for the native runner: WebSocket client connections.
//!
//! Each connection runs on a background thread; the guest-facing API is
//! non-blocking (queues + an atomic state), matching the browser runner.
//! `ws://` and `wss://` (rustls, trusting the OS certificate store; set
//! `SSL_CERT_FILE` to use a specific CA bundle instead).
//!
//! Limits keep a misbehaving guest or peer from exhausting the host: at most
//! [`MAX_CONNECTIONS`] open at once, bounded queues in both directions (a full
//! send queue makes `send` fail; a full receive queue stops reading the socket,
//! so TCP pushes back on the peer), and timeouts for connecting and the handshake.

use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub const CONNECTING: u32 = 0;
pub const OPEN: u32 = 1;
pub const CLOSED: u32 = 2;
pub const ERROR: u32 = 3;

pub const MAX_CONNECTIONS: usize = 16;
/// Messages queued per direction per connection.
const QUEUE: usize = 4096;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

struct Conn {
    state: Arc<AtomicU32>,
    outgoing: SyncSender<Vec<u8>>,
    thread: Option<std::thread::JoinHandle<()>>,
    incoming: Receiver<Vec<u8>>,
    queue: VecDeque<Vec<u8>>,
}

/// The hosts guests may reach (gasm:net and gasm:fetch): none, all (`--allow-net`)
/// or a list (`--allow-net=api.met.no,*.example.org`; `*.` matches subdomains).
#[derive(Clone, Debug, Default)]
pub struct NetPolicy {
    pub allowed: bool,
    /// empty: every host (when allowed)
    pub hosts: Vec<String>,
}

impl NetPolicy {
    pub fn new(allowed: bool, hosts: Vec<String>) -> NetPolicy {
        NetPolicy { allowed, hosts: hosts.into_iter().map(|h| h.to_ascii_lowercase()).collect() }
    }

    pub fn permits(&self, host: &str) -> bool {
        if !self.allowed {
            return false;
        }
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.is_empty()
            || self.hosts.iter().any(|p| match p.strip_prefix("*.") {
                Some(domain) => host.len() > domain.len() + 1 && host.ends_with(domain) && host.as_bytes()[host.len() - domain.len() - 1] == b'.',
                None => host == *p,
            })
    }

    /// Why a URL is refused (for the log), or None if it may be reached.
    pub fn refusal(&self, url: &str) -> Option<String> {
        if !self.allowed {
            return Some("run with --allow-net".into());
        }
        let host = url_host(url).unwrap_or_default();
        (!self.permits(&host)).then(|| format!("{host} is not in --allow-net={}", self.hosts.join(",")))
    }
}

/// The host of an absolute URL (no userinfo, no port, brackets kept for IPv6).
pub fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let host = if authority.starts_with('[') {
        &authority[..=authority.find(']')?]
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

pub struct Net {
    policy: NetPolicy,
    conns: HashMap<i32, Conn>,
    next: i32,
}

impl Net {
    pub fn new(allowed: bool) -> Net {
        Net::with_policy(NetPolicy::new(allowed, Vec::new()))
    }

    pub fn with_policy(policy: NetPolicy) -> Net {
        Net { policy, conns: HashMap::new(), next: 1 }
    }

    pub fn open(&mut self, url: &str) -> i32 {
        if let Some(why) = self.policy.refusal(url) {
            eprintln!("[gasm] net: denied connection to {url} ({why})");
            return -1;
        }
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            eprintln!("[gasm] net: only ws:// and wss:// URLs are supported: {url}");
            return -1;
        }
        if self.conns.len() >= MAX_CONNECTIONS {
            eprintln!("[gasm] net: too many connections (max {MAX_CONNECTIONS}): {url}");
            return -1;
        }
        install_crypto_provider();
        let state = Arc::new(AtomicU32::new(CONNECTING));
        let (out_tx, out_rx) = mpsc::sync_channel::<Vec<u8>>(QUEUE);
        let (in_tx, in_rx) = mpsc::sync_channel::<Vec<u8>>(QUEUE);
        let (st, url_owned) = (state.clone(), url.to_owned());
        let thread = std::thread::spawn(move || run(&url_owned, &st, out_rx, in_tx));
        let h = self.next;
        self.next += 1;
        self.conns.insert(h, Conn { state, outgoing: out_tx, incoming: in_rx, queue: VecDeque::new(), thread: Some(thread) });
        h
    }

    /// A handle `open` never returned is an error (the guest traps); a closed
    /// one is `Ok(false)`.
    fn check(&self, h: i32) -> Result<bool, String> {
        if h <= 0 || h >= self.next {
            return Err(format!("gasm:net: invalid connection handle {h}"));
        }
        Ok(self.conns.contains_key(&h))
    }

    pub fn state(&self, h: i32) -> Result<u32, String> {
        Ok(match self.check(h)? {
            true => self.conns[&h].state.load(Ordering::Acquire),
            false => CLOSED,
        })
    }

    /// 0, or -1 if not open or the send queue is full.
    pub fn send(&mut self, h: i32, msg: Vec<u8>) -> Result<i32, String> {
        if !self.check(h)? {
            return Ok(-1);
        }
        let c = &self.conns[&h];
        if c.state.load(Ordering::Acquire) != OPEN {
            return Ok(-1);
        }
        Ok(match c.outgoing.try_send(msg) {
            Ok(()) => 0,
            Err(TrySendError::Full(_)) => {
                eprintln!("[gasm] net: send queue full ({QUEUE} messages)");
                -1
            }
            Err(TrySendError::Disconnected(_)) => -1,
        })
    }

    /// Next message without removing it; Ok(Err(0)) = none yet, Ok(Err(-1)) = closed and drained.
    pub fn peek(&mut self, h: i32) -> Result<Result<&[u8], i32>, String> {
        if !self.check(h)? {
            return Ok(Err(-1));
        }
        Ok(self.peek_open(h))
    }

    fn peek_open(&mut self, h: i32) -> Result<&[u8], i32> {
        let Some(c) = self.conns.get_mut(&h) else { return Err(-1) };
        while let Ok(m) = c.incoming.try_recv() {
            c.queue.push_back(m);
        }
        if c.queue.is_empty() {
            return Err(if c.state.load(Ordering::Acquire) >= CLOSED { -1 } else { 0 });
        }
        Ok(c.queue.front().unwrap())
    }

    pub fn pop(&mut self, h: i32) {
        if let Some(c) = self.conns.get_mut(&h) {
            c.queue.pop_front();
        }
    }

    /// Closing a closed connection does nothing.
    pub fn close(&mut self, h: i32) -> Result<(), String> {
        self.check(h)?;
        self.conns.remove(&h); // dropping the sender ends the thread (after flushing)
        Ok(())
    }
}

impl Drop for Net {
    /// Let connection threads flush what the guest already sent (e.g. the last
    /// lockstep inputs before a game exits) instead of dropping it on exit.
    fn drop(&mut self) {
        let threads: Vec<_> = self.conns.drain().filter_map(|(_, mut c)| c.thread.take()).collect();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        for t in threads {
            while !t.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// Connection thread: connect, then alternate between flushing outgoing
/// messages and reading with a short timeout.
fn run(url: &str, state: &AtomicU32, outgoing: Receiver<Vec<u8>>, incoming: SyncSender<Vec<u8>>) {
    let mut ws = match connect(url) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[gasm] net: {url}: {e}");
            state.store(ERROR, Ordering::Release);
            return;
        }
    };
    // Non-blocking after the handshake: one thread alternates between sending and
    // receiving. (Not read timeouts: on Windows a timed-out receive leaves the
    // socket in an indeterminate state.)
    let tcp = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => Some(&*s),
        MaybeTlsStream::Rustls(s) => Some(&s.sock),
        _ => None,
    };
    if let Some(tcp) = tcp {
        let _ = tcp.set_nodelay(true);
        if let Err(e) = tcp.set_nonblocking(true) {
            eprintln!("[gasm] net: {url}: {e}");
            state.store(ERROR, Ordering::Release);
            return;
        }
    }
    state.store(OPEN, Ordering::Release);
    let end = pump(&mut ws, &outgoing, &incoming);
    if let Err(e) = &end {
        eprintln!("[gasm] net: {url}: {e}");
    }
    state.store(if end.is_ok() { CLOSED } else { ERROR }, Ordering::Release);
    close_gracefully(&mut ws);
}

/// TCP connect and TLS/WebSocket handshakes, each bounded by [`CONNECT_TIMEOUT`].
fn connect(url: &str) -> Result<WebSocket<MaybeTlsStream<std::net::TcpStream>>, String> {
    use std::net::ToSocketAddrs;
    let uri: tungstenite::http::Uri = url.parse().map_err(|e| format!("bad URL: {e}"))?;
    let host = uri.host().ok_or("URL has no host")?.trim_start_matches('[').trim_end_matches(']');
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("wss") { 443 } else { 80 });
    let mut last = String::from("no address");
    for addr in (host, port).to_socket_addrs().map_err(|e| e.to_string())? {
        match std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(CONNECT_TIMEOUT)).map_err(|e| e.to_string())?;
                tcp.set_write_timeout(Some(CONNECT_TIMEOUT)).map_err(|e| e.to_string())?;
                let (ws, _) = tungstenite::client_tls(url, tcp).map_err(|e| e.to_string())?;
                let tcp = match ws.get_ref() {
                    MaybeTlsStream::Plain(s) => s,
                    MaybeTlsStream::Rustls(s) => &s.sock,
                    _ => return Ok(ws),
                };
                tcp.set_read_timeout(None).map_err(|e| e.to_string())?;
                tcp.set_write_timeout(None).map_err(|e| e.to_string())?;
                return Ok(ws);
            }
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    Err(last)
}

/// WebSocket close handshake, draining incoming data until the peer confirms.
/// Exiting with unread data makes the OS reset the connection, and a reset can
/// make the other side discard messages it hasn't read yet (e.g. our last inputs).
fn close_gracefully(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) {
    if ws.close(None).is_err() {
        return;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        match ws.read() {
            Ok(_) => {}
            Err(e) if would_block(&e) => std::thread::sleep(Duration::from_millis(1)),
            Err(_) => break, // ConnectionClosed: handshake complete
        }
    }
}

/// rustls needs a process-wide crypto backend; we use ring (see Cargo.toml).
pub fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted))
}

/// Non-blocking pump: flush outgoing messages, read what's there, and sleep
/// briefly only when idle. A send that can't complete now stays buffered in
/// tungstenite and goes out with the next flush.
pub(crate) fn pump(
    ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    outgoing: &Receiver<Vec<u8>>,
    incoming: &SyncSender<Vec<u8>>,
) -> Result<(), String> {
    // a message the guest's full receive queue couldn't take yet: read nothing more until it fits
    let mut pending: Option<Vec<u8>> = None;
    loop {
        let mut busy = false;
        loop {
            match outgoing.try_recv() {
                Ok(m) => {
                    busy = true;
                    match ws.send(Message::Binary(m.into())) {
                        Ok(()) => {}
                        Err(e) if would_block(&e) => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    flush_all(ws);
                    return Ok(()); // guest closed it
                }
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(e) => return Err(e.to_string()),
        }
        if let Some(m) = pending.take() {
            match incoming.try_send(m) {
                Ok(()) => busy = true,
                Err(TrySendError::Full(m)) => pending = Some(m),
                Err(TrySendError::Disconnected(_)) => return Ok(()),
            }
        }
        let read = if pending.is_some() { Err(tungstenite::Error::Io(ErrorKind::WouldBlock.into())) } else { ws.read() };
        match read {
            Ok(Message::Binary(b)) => {
                busy = true;
                pending = Some(b.to_vec());
            }
            Ok(Message::Text(t)) => {
                busy = true;
                pending = Some(t.as_bytes().to_vec());
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => busy = true,
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
        if !busy {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Push out anything still buffered (bounded), e.g. before closing.
fn flush_all(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        match ws.flush() {
            Err(e) if would_block(&e) => std::thread::sleep(Duration::from_millis(1)),
            _ => return,
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(url_host("https://User@API.met.no:443/x?y").as_deref(), Some("api.met.no"));
        assert_eq!(url_host("http://[::1]:8080/").as_deref(), Some("[::1]"));
        assert_eq!(url_host("ws://localhost").as_deref(), Some("localhost"));
        let p = NetPolicy::new(true, vec!["api.met.no".into(), "*.example.org".into()]);
        assert!(p.permits("api.met.no") && p.permits("API.MET.NO.") && p.permits("a.b.example.org"));
        assert!(!p.permits("met.no") && !p.permits("example.org") && !p.permits("badexample.org"));
        assert!(NetPolicy::new(true, vec![]).permits("anything"));
        assert!(!NetPolicy::new(false, vec![]).permits("anything"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wait (up to 10 s) for a connection to leave CONNECTING; return its state.
    fn settle(net: &Net, h: i32) -> u32 {
        let t0 = std::time::Instant::now();
        while net.state(h).unwrap() == CONNECTING && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        net.state(h).unwrap()
    }

    /// Public TLS endpoint, OS trust store. Run with: cargo test --release -- --ignored
    #[test]
    #[ignore = "needs internet"]
    fn wss_public_echo() {
        let mut net = Net::new(true);
        let h = net.open("wss://echo.websocket.org");
        assert!(h > 0);
        assert_eq!(settle(&net, h), OPEN, "TLS connection did not open");
        assert_eq!(net.send(h, b"gasm-tls-check".to_vec()), Ok(0));
        let t0 = std::time::Instant::now();
        let mut echoed = false;
        while t0.elapsed() < Duration::from_secs(10) && !echoed {
            match net.peek(h).unwrap() {
                Ok(m) => {
                    echoed = m == b"gasm-tls-check";
                    net.pop(h);
                }
                Err(0) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        assert!(echoed, "no echo over wss://");
    }

    #[test]
    fn denied_without_permission() {
        assert_eq!(Net::new(false).open("wss://example.com"), -1);
        assert_eq!(Net::new(true).open("http://example.com"), -1);
    }

    #[test]
    fn unknown_handles_are_errors_closed_ones_are_not() {
        let mut net = Net::new(true);
        assert!(net.state(1).is_err() && net.state(0).is_err() && net.close(7).is_err());
        let h = net.open("ws://127.0.0.1:1"); // nothing listens on port 1
        assert!(h > 0);
        net.close(h).unwrap();
        assert_eq!(net.state(h), Ok(CLOSED));
        assert_eq!(net.send(h, vec![1]), Ok(-1));
        assert_eq!(net.peek(h).unwrap().err(), Some(-1));
        net.close(h).unwrap(); // again: nothing happens
    }
}
