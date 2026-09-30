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
//! Membership events are sent before any data from the new peer, so every
//! client sees the same order of events. The relay never inspects payloads.

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tungstenite::handshake::server::{Request, Response};
use tungstenite::{Message, WebSocket};

const WELCOME: u8 = 1;
const JOIN: u8 = 2;
const LEAVE: u8 = 3;
const FULL: u8 = 4;
const DATA: u8 = 0x10;

type Rooms = Arc<Mutex<HashMap<String, Vec<Option<Sender<Vec<u8>>>>>>>;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut addr = "0.0.0.0:9000".to_string();
    let mut max_peers = 2usize;
    let (mut cert, mut key) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--max-peers" => max_peers = args.next().and_then(|v| v.parse().ok()).unwrap_or(2).clamp(1, 255),
            "--tls-cert" => cert = args.next(),
            "--tls-key" => key = args.next(),
            "-h" | "--help" => {
                eprintln!("usage: gasm-relay [addr:port] [--max-peers N] [--tls-cert chain.pem --tls-key key.pem]");
                return;
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
    for stream in listener.incoming().flatten() {
        let (rooms, tls) = (rooms.clone(), tls.clone());
        std::thread::spawn(move || {
            if let Err(e) = handle(stream, &rooms, max_peers, tls) {
                eprintln!("[relay] client error: {e}");
            }
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

/// The TCP socket under a (possibly TLS) stream: timeouts must be set on the
/// socket actually in use. (A try_clone()'d handle doesn't share SO_RCVTIMEO
/// on Windows, which deadlocked relay threads in read.)
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
    match tls {
        None => client(tcp, rooms, max_peers),
        Some(cfg) => {
            let conn = rustls::ServerConnection::new(cfg).map_err(|e| e.to_string())?;
            client(rustls::StreamOwned::new(conn, tcp), rooms, max_peers)
        }
    }
}

fn broadcast(rooms: &Rooms, room: &str, except: usize, msg: &[u8]) {
    if let Some(peers) = rooms.lock().unwrap().get(room) {
        for (i, p) in peers.iter().enumerate() {
            if let (true, Some(tx)) = (i != except, p) {
                let _ = tx.send(msg.to_vec());
            }
        }
    }
}

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
    // After the (blocking) handshakes: short read timeouts let one thread alternate
    // between forwarding queued messages and reading.
    ws.get_ref().tcp().set_read_timeout(Some(Duration::from_millis(2))).map_err(|e| e.to_string())?;

    // Join: take the lowest free index, tell the others while holding the lock
    // so membership events are ordered before any data from this peer.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
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
        for (j, p) in peers.iter().enumerate() {
            if let (true, Some(other)) = (j != i, p) {
                let _ = other.send(vec![JOIN, i as u8]);
            }
        }
        peers[i] = Some(tx);
        let count = peers.iter().filter(|p| p.is_some()).count();
        let _ = ws.send(Message::Binary(vec![WELCOME, i as u8, count as u8].into()));
        i
    };
    eprintln!("[relay] {peer_addr} joined room {room:?} as peer {index}");

    let result = pump(&mut ws, &rx, rooms, &room, index);

    {
        let mut map = rooms.lock().unwrap();
        if let Some(peers) = map.get_mut(&room) {
            peers[index] = None;
            if peers.iter().all(Option::is_none) {
                map.remove(&room);
            }
        }
    }
    broadcast(rooms, &room, index, &[LEAVE, index as u8]);
    eprintln!("[relay] peer {index} left room {room:?}");
    let _ = ws.close(None);
    result
}

fn pump<S: Read + Write>(ws: &mut WebSocket<S>, rx: &Receiver<Vec<u8>>, rooms: &Rooms, room: &str, index: usize) -> Result<(), String> {
    loop {
        loop {
            match rx.try_recv() {
                Ok(m) => ws.send(Message::Binary(m.into())).map_err(|e| e.to_string())?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        match ws.read() {
            Ok(Message::Binary(b)) if b.first() == Some(&DATA) => {
                let mut out = Vec::with_capacity(b.len() + 1);
                out.extend_from_slice(&[DATA, index as u8]);
                out.extend_from_slice(&b[1..]);
                broadcast(rooms, room, index, &out);
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
}
