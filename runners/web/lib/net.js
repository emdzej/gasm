// gasm:net over the platform WebSocket (browsers, Node >= 22). Handles > 0; messages
// are binary. Same rules as runners/native/src/net.rs: a handle `open` never returned
// traps, a closed one reports CLOSED; at most MAX_CONNECTIONS open at once.

export const NET_CONNECTING = 0, NET_OPEN = 1, NET_CLOSED = 2, NET_ERROR = 3;
export const MAX_CONNECTIONS = 16;
const ENC = new TextEncoder();

export class NetConnections {
  constructor(allowed, log) { this.allowed = allowed; this.log = log; this.conns = new Map(); this.next = 1; }

  /** Is h a handle `open` returned? Throws (traps) if not; false once closed. */
  check(h) {
    if (!Number.isInteger(h) || h <= 0 || h >= this.next) throw new Error(`gasm:net: invalid connection handle ${h}`);
    return this.conns.has(h);
  }

  open(url) {
    if (!this.allowed) { this.log(`[gasm] net: denied connection to ${url} (networking not enabled)`); return -1; }
    if (!/^wss?:\/\//.test(url) || typeof WebSocket === 'undefined') { this.log(`[gasm] net: only ws:// and wss:// URLs are supported: ${url}`); return -1; }
    if (this.conns.size >= MAX_CONNECTIONS) { this.log(`[gasm] net: too many connections (max ${MAX_CONNECTIONS}): ${url}`); return -1; }
    let ws;
    try { ws = new WebSocket(url); } catch (e) { this.log(`[gasm] net: ${url}: ${e.message}`); return -1; }
    ws.binaryType = 'arraybuffer';
    const c = { ws, queue: [], state: NET_CONNECTING };
    ws.onopen = () => { c.state = NET_OPEN; };
    ws.onmessage = (e) => c.queue.push(e.data instanceof ArrayBuffer ? new Uint8Array(e.data) : ENC.encode(String(e.data)));
    ws.onerror = () => { if (c.state < NET_CLOSED) c.state = NET_ERROR; };
    ws.onclose = () => { if (c.state !== NET_ERROR) c.state = NET_CLOSED; };
    const h = this.next++;
    this.conns.set(h, c);
    return h;
  }

  state(h) { return this.check(h) ? this.conns.get(h).state : NET_CLOSED; }

  send(h, bytes) {
    if (!this.check(h)) return -1;
    const c = this.conns.get(h);
    if (c.state !== NET_OPEN) return -1;
    c.ws.send(bytes);
    return 0;
  }

  /** Next message (stays queued), 0 = none yet, -1 = closed and drained. */
  peek(h) {
    if (!this.check(h)) return -1;
    const c = this.conns.get(h);
    if (c.queue.length) return c.queue[0];
    return c.state >= NET_CLOSED ? -1 : 0;
  }

  pop(h) { this.conns.get(h)?.queue.shift(); }

  close(h) {
    if (!this.check(h)) return;
    this.conns.get(h).ws.close();
    this.conns.delete(h);
  }

  /** Close every connection with a proper handshake and wait (bounded) for it, so
   *  queued messages are delivered before the game goes away. */
  closeAll(timeoutMs = 1000) {
    const waits = [...this.conns.values()].map((c) => new Promise((resolve) => {
      if (c.ws.readyState >= 2) return resolve();
      c.ws.addEventListener('close', resolve, { once: true });
      c.ws.close();
    }));
    this.conns.clear();
    return Promise.race([Promise.all(waits), new Promise((r) => setTimeout(r, timeoutMs))]);
  }
}
