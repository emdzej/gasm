//! gasm:net for the native runner: WebSocket client connections.
//!
//! Each connection runs on a background thread; the guest-facing API is
//! non-blocking (queues + an atomic state), matching the browser runner.
//! Only `ws://` is supported natively for now (no TLS).

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
        if !url.starts_with("ws://") {
            eprintln!("[gasm] net: only ws:// URLs are supported natively: {url}");
            return -1;
        }
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
    if let MaybeTlsStream::Plain(s) = ws.get_mut() {
        let _ = s.set_nodelay(true);
        let _ = s.set_read_timeout(Some(Duration::from_millis(2)));
    }
    state.store(OPEN, Ordering::Release);
    let end = pump(&mut ws, &outgoing, &incoming);
    state.store(if end.is_ok() { CLOSED } else { ERROR }, Ordering::Release);
    let _ = ws.close(None);
}

pub(crate) fn pump(
    ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    outgoing: &Receiver<Vec<u8>>,
    incoming: &Sender<Vec<u8>>,
) -> Result<(), String> {
    loop {
        loop {
            match outgoing.try_recv() {
                Ok(m) => ws.send(Message::Binary(m.into())).map_err(|e| e.to_string())?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()), // guest closed it
            }
        }
        match ws.read() {
            Ok(Message::Binary(b)) => {
                if incoming.send(b.to_vec()).is_err() {
                    return Ok(());
                }
            }
            Ok(Message::Text(t)) => {
                let _ = incoming.send(t.as_bytes().to_vec());
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
}
