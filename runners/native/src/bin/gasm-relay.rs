//! gasm-relay: a tiny, game-agnostic WebSocket room relay.
//!
//!   gasm-relay [addr:port]            (default 0.0.0.0:9000)
//!   gasm-relay 0.0.0.0:9000 --max-peers 4
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
use std::io::ErrorKind;
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
    while let Some(a) = args.next() {
        match a.as_str() {
            "--max-peers" => max_peers = args.next().and_then(|v| v.parse().ok()).unwrap_or(2).clamp(1, 255),
            "-h" | "--help" => {
                eprintln!("usage: gasm-relay [addr:port] [--max-peers N]");
                return;
            }
            _ => addr = a,
        }
    }
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("gasm-relay: cannot listen on {addr}: {e}");
        std::process::exit(1);
    });
    eprintln!("[relay] listening on ws://{addr}/<room> (max {max_peers} peers per room)");
    let rooms: Rooms = Arc::default();
    for stream in listener.incoming().flatten() {
        let rooms = rooms.clone();
        std::thread::spawn(move || {
            if let Err(e) = client(stream, &rooms, max_peers) {
                eprintln!("[relay] client error: {e}");
            }
        });
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

fn client(stream: TcpStream, rooms: &Rooms, max_peers: usize) -> Result<(), String> {
    let peer_addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
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
    let _ = ws.get_mut().set_nodelay(true);
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(2))).map_err(|e| e.to_string())?;

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

fn pump(ws: &mut WebSocket<TcpStream>, rx: &Receiver<Vec<u8>>, rooms: &Rooms, room: &str, index: usize) -> Result<(), String> {
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
