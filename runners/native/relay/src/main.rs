//! gasm-relay: a tiny, game-agnostic WebSocket room relay.
//!
//!   gasm-relay [addr:port]            (default 0.0.0.0:9000)
//!   gasm-relay 0.0.0.0:9000 --max-peers 4
//!   gasm-relay 0.0.0.0:9443 --tls-cert fullchain.pem --tls-key privkey.pem   (wss://)
//!
//! Clients connect to ws://host:port/<room>. Binary protocol (byte 0 = type):
//!
//!   relay -> client  [1 WELCOME][u8 your_index][u8 peers_in_room]
//!                    [2 JOIN][u8 index]        another peer joined
//!                    [3 LEAVE][u8 index]       a peer left
//!                    [4 FULL]                  room full; connection closes
//!                    [0x10 DATA][u8 from][payload...]
//!   client -> relay  [0x10 DATA][payload...]   broadcast to the other peers
//!
//! Membership events are sent (under the room lock) before any data from the
//! new peer and before the slot can be reused, so every client sees the same
//! order of events. The relay never inspects payloads.
//!
//! Limits: at most `--max-clients` connections (default 256), a 10 s timeout for
//! the TLS and WebSocket handshakes, and a bounded queue per peer (a peer that
//! stops reading is disconnected instead of buffering everyone's traffic).

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Message, WebSocket};

const WELCOME: u8 = 1;
const JOIN: u8 = 2;
const LEAVE: u8 = 3;
const FULL: u8 = 4;
const DATA: u8 = 0x10;

/// Messages queued for one peer before it counts as stuck.
const PEER_QUEUE: usize = 4096;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

type Peer = SyncSender<Vec<u8>>;
type Rooms = Arc<Mutex<HashMap<String, Vec<Option<Peer>>>>>;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut addr = "0.0.0.0:9000".to_string();
    let mut max_peers = 2usize;
    let mut max_clients = 256usize;
    let (mut cert, mut key) = (None, None);
    let usage = "usage: gasm-relay [addr:port] [--max-peers N] [--max-clients N] [--tls-cert chain.pem --tls-key key.pem]";
    let number = |v: Option<String>, name: &str| -> usize {
        v.and_then(|v| v.parse().ok()).unwrap_or_else(|| {
            eprintln!("gasm-relay: {name} expects a number\n{usage}");
            std::process::exit(2);
        })
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--max-peers" => max_peers = number(args.next(), "--max-peers").clamp(1, 255),
            "--max-clients" => max_clients = number(args.next(), "--max-clients").max(1),
            "--tls-cert" => cert = args.next(),
            "--tls-key" => key = args.next(),
            "-h" | "--help" => {
                eprintln!("{usage}");
                return;
            }
            s if s.starts_with('-') => {
                eprintln!("gasm-relay: unknown option {s}\n{usage}");
                std::process::exit(2);
            }
            _ => addr = a,
        }
    }
    let tls = match (cert, key) {
        (Some(c), Some(k)) => Some(load_tls(&c, &k).unwrap_or_else(|e| {
            eprintln!("gasm-relay: TLS: {e}");
            std::process::exit(1);
        })),
        (None, None) => None,
        _ => {
            eprintln!("gasm-relay: --tls-cert and --tls-key go together");
            std::process::exit(2);
        }
    };
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("gasm-relay: cannot listen on {addr}: {e}");
        std::process::exit(1);
    });
    let scheme = if tls.is_some() { "wss" } else { "ws" };
    eprintln!("[relay] listening on {scheme}://{addr}/<room> (max {max_peers} peers per room)");
    let rooms: Rooms = Arc::default();
    let clients = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if clients.load(Ordering::Acquire) >= max_clients {
            eprintln!("[relay] refusing {:?}: {max_clients} clients connected", stream.peer_addr().ok());
            continue; // dropping the stream closes it
        }
        clients.fetch_add(1, Ordering::AcqRel);
        let (rooms, tls, clients) = (rooms.clone(), tls.clone(), clients.clone());
        std::thread::spawn(move || {
            if let Err(e) = handle(stream, &rooms, max_peers, tls) {
                eprintln!("[relay] client error: {e}");
            }
            clients.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

fn load_tls(cert: &str, key: &str) -> Result<Arc<rustls::ServerConfig>, String> {
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certs = CertificateDer::pem_file_iter(cert)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("{cert}: {e}"))?;
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| format!("{key}: {e}"))?;
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(cfg))
}

/// The TCP socket under a (possibly TLS) stream, to switch it to non-blocking
/// after the handshakes. (Read timeouts are unreliable on Windows: a timed-out
/// receive leaves the socket indeterminate, and cloned handles don't share them.)
trait Socket: Read + Write {
    fn tcp(&self) -> &TcpStream;
}
impl Socket for TcpStream {
    fn tcp(&self) -> &TcpStream {
        self
    }
}
impl Socket for rustls::StreamOwned<rustls::ServerConnection, TcpStream> {
    fn tcp(&self) -> &TcpStream {
        &self.sock
    }
}

/// Plain or TLS: the WebSocket code is generic over the stream.
fn handle(tcp: TcpStream, rooms: &Rooms, max_peers: usize, tls: Option<Arc<rustls::ServerConfig>>) -> Result<(), String> {
    let _ = tcp.set_nodelay(true);
    // the handshakes block: bound them (a client that never finishes is dropped)
    tcp.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).map_err(|e| e.to_string())?;
    tcp.set_write_timeout(Some(HANDSHAKE_TIMEOUT)).map_err(|e| e.to_string())?;
    match tls {
        None => client(tcp, rooms, max_peers),
        Some(cfg) => {
            let conn = rustls::ServerConnection::new(cfg).map_err(|e| e.to_string())?;
            client(rustls::StreamOwned::new(conn, tcp), rooms, max_peers)
        }
    }
}

/// Send to every other peer of the room. A peer whose queue is full is stuck:
/// it is dropped from the room (its thread then disconnects it).
fn send_others(peers: &mut [Option<Peer>], except: usize, msg: &[u8]) {
    for (i, p) in peers.iter_mut().enumerate() {
        if i == except {
            continue;
        }
        if let Some(tx) = p
            && let Err(TrySendError::Full(_)) = tx.try_send(msg.to_vec())
        {
            eprintln!("[relay] peer {i} is not reading; disconnecting it");
            *p = None;
        }
    }
}

fn broadcast(rooms: &Rooms, room: &str, except: usize, msg: &[u8]) {
    if let Some(peers) = rooms.lock().unwrap().get_mut(room) {
        send_others(peers, except, msg);
    }
}

#[allow(clippy::result_large_err)]
fn client<S: Socket>(stream: S, rooms: &Rooms, max_peers: usize) -> Result<(), String> {
    let peer_addr = stream.tcp().peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let mut path = String::new();
    let mut ws = tungstenite::accept_hdr(stream, |req: &Request, resp: Response| {
        path = req.uri().path().to_string();
        Ok(resp)
    })
    .map_err(|e| e.to_string())?;
    let room = match path.trim_matches('/') {
        "" => "lobby".to_string(),
        r => r.to_string(),
    };
    // After the (blocking) handshakes: non-blocking, so one thread alternates
    // between forwarding queued messages and reading.
    let tcp = ws.get_ref().tcp();
    tcp.set_read_timeout(None).map_err(|e| e.to_string())?;
    tcp.set_write_timeout(None).map_err(|e| e.to_string())?;
    tcp.set_nonblocking(true).map_err(|e| e.to_string())?;

    // Join: take the lowest free index, tell the others while holding the lock
    // so membership events are ordered before any data from this peer.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(PEER_QUEUE);
    let index = {
        let mut map = rooms.lock().unwrap();
        let peers = map.entry(room.clone()).or_default();
        let free = peers.iter().position(Option::is_none).or((peers.len() < max_peers).then_some(peers.len()));
        let Some(i) = free else {
            drop(map);
            let _ = ws.send(Message::Binary(vec![FULL].into()));
            let _ = ws.close(None);
            eprintln!("[relay] {peer_addr} rejected: room {room:?} is full");
            return Ok(());
        };
        if i == peers.len() {
            peers.push(None);
        }
        send_others(peers, i, &[JOIN, i as u8]);
        peers[i] = Some(tx);
        let count = peers.iter().filter(|p| p.is_some()).count();
        let _ = ws.send(Message::Binary(vec![WELCOME, i as u8, count as u8].into()));
        i
    };
    eprintln!("[relay] {peer_addr} joined room {room:?} as peer {index}");

    let result = pump(&mut ws, &rx, rooms, &room, index);

    // Leave: free the slot and tell the others under the same lock, so a peer
    // joining into this slot is announced after the LEAVE.
    {
        let mut map = rooms.lock().unwrap();
        if let Some(peers) = map.get_mut(&room) {
            peers[index] = None;
            send_others(peers, index, &[LEAVE, index as u8]);
            if peers.iter().all(Option::is_none) {
                map.remove(&room);
            }
        }
    }
    eprintln!("[relay] peer {index} left room {room:?}");
    let _ = ws.close(None);
    result
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted))
}

fn pump<S: Read + Write>(ws: &mut WebSocket<S>, rx: &Receiver<Vec<u8>>, rooms: &Rooms, room: &str, index: usize) -> Result<(), String> {
    loop {
        let mut busy = false;
        loop {
            match rx.try_recv() {
                Ok(m) => {
                    busy = true;
                    match ws.send(Message::Binary(m.into())) {
                        Ok(()) => {}
                        Err(e) if would_block(&e) => {} // buffered; flushed below
                        Err(e) => return Err(e.to_string()),
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(e) => return Err(e.to_string()),
        }
        match ws.read() {
            Ok(Message::Binary(b)) if b.first() == Some(&DATA) => {
                busy = true;
                let mut out = Vec::with_capacity(b.len() + 1);
                out.extend_from_slice(&[DATA, index as u8]);
                out.extend_from_slice(&b[1..]);
                broadcast(rooms, room, index, &out);
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
