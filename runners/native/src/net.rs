//! gasm:net for the native runner: WebSocket client connections.
//!
//! Each connection runs on a background thread; the guest-facing API is
//! non-blocking (queues + an atomic state), matching the browser runner.
//! `ws://` and `wss://` (rustls, trusting the OS certificate store; set
//! `SSL_CERT_FILE` to use a specific CA bundle instead).

use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub const CONNECTING: u32 = 0;
pub const OPEN: u32 = 1;
pub const CLOSED: u32 = 2;
pub const ERROR: u32 = 3;

struct Conn {
    state: Arc<AtomicU32>,
    outgoing: Sender<Vec<u8>>,
    thread: Option<std::thread::JoinHandle<()>>,
    incoming: Receiver<Vec<u8>>,
    queue: VecDeque<Vec<u8>>,
}

pub struct Net {
    allowed: bool,
    conns: HashMap<i32, Conn>,
    next: i32,
}

impl Net {
    pub fn new(allowed: bool) -> Net {
        Net { allowed, conns: HashMap::new(), next: 1 }
    }

    pub fn open(&mut self, url: &str) -> i32 {
        if !self.allowed {
            eprintln!("[gasm] net: denied connection to {url} (run with --allow-net)");
            return -1;
        }
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            eprintln!("[gasm] net: only ws:// and wss:// URLs are supported: {url}");
            return -1;
        }
        install_crypto_provider();
        let state = Arc::new(AtomicU32::new(CONNECTING));
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();
        let (st, url_owned) = (state.clone(), url.to_owned());
        let thread = std::thread::spawn(move || run(&url_owned, &st, out_rx, in_tx));
        let h = self.next;
        self.next += 1;
        self.conns.insert(h, Conn { state, outgoing: out_tx, incoming: in_rx, queue: VecDeque::new(), thread: Some(thread) });
        h
    }

    pub fn state(&self, h: i32) -> u32 {
        self.conns.get(&h).map_or(ERROR, |c| c.state.load(Ordering::Acquire))
    }

    pub fn send(&mut self, h: i32, msg: Vec<u8>) -> i32 {
        match self.conns.get(&h) {
            Some(c) if c.state.load(Ordering::Acquire) == OPEN && c.outgoing.send(msg).is_ok() => 0,
            _ => -1,
        }
    }

    /// Next message without removing it; Err(0) = none yet, Err(-1) = closed and drained.
    pub fn peek(&mut self, h: i32) -> Result<&[u8], i32> {
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

    pub fn close(&mut self, h: i32) {
        self.conns.remove(&h); // dropping the sender ends the thread
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
fn run(url: &str, state: &AtomicU32, outgoing: Receiver<Vec<u8>>, incoming: Sender<Vec<u8>>) {
    let mut ws = match tungstenite::connect(url) {
        Ok((ws, _)) => ws,
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
    incoming: &Sender<Vec<u8>>,
) -> Result<(), String> {
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
        match ws.read() {
            Ok(Message::Binary(b)) => {
                busy = true;
                if incoming.send(b.to_vec()).is_err() {
                    return Ok(());
                }
            }
            Ok(Message::Text(t)) => {
                busy = true;
                let _ = incoming.send(t.as_bytes().to_vec());
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
mod tests {
    use super::*;

    /// Wait (up to 10 s) for a connection to leave CONNECTING; return its state.
    fn settle(net: &Net, h: i32) -> u32 {
        let t0 = std::time::Instant::now();
        while net.state(h) == CONNECTING && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        net.state(h)
    }

    /// Public TLS endpoint, OS trust store. Run with: cargo test --release -- --ignored
    #[test]
    #[ignore = "needs internet"]
    fn wss_public_echo() {
        let mut net = Net::new(true);
        let h = net.open("wss://echo.websocket.org");
        assert!(h > 0);
        assert_eq!(settle(&net, h), OPEN, "TLS connection did not open");
        assert_eq!(net.send(h, b"gasm-tls-check".to_vec()), 0);
        let t0 = std::time::Instant::now();
        let mut echoed = false;
        while t0.elapsed() < Duration::from_secs(10) && !echoed {
            match net.peek(h) {
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
}
