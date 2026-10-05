// gasm:fetch over the platform fetch() (browsers, Node >= 22): HTTP requests made
// for the guest (design/fetch.md). Same rules as runners/native/src/fetch.rs: the
// same descriptions are accepted and refused, a handle `request` never returned
// traps, a closed one reports FAILED, at most MAX_REQUESTS open at once. Browsers
// add their own rules (CORS, mixed content). Redirects are followed by fetch(); the
// final URL's host must be allowed too.
//
// Headless runs can record responses and replay them (`replay`/`record` options):
// a replayed request completes at the start of the next frame, as natively, with the
// same record key and files.

export const FETCH_PENDING = 0, FETCH_HEADERS = 1, FETCH_DONE = 2, FETCH_FAILED = 3;
export const MAX_REQUESTS = 16;
export const MAX_BODY = 64 * 1024 * 1024;
export const MAX_REQUEST_BODY = 16 * 1024 * 1024;
const QUEUED = 4 * 1024 * 1024;   // body bytes buffered before reading pauses
const TIMEOUT_MS = 120_000;
export const METHODS = ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'];
// The Fetch standard's forbidden request headers, plus user-agent (same list natively)
export const FORBIDDEN_HEADERS = [
  'accept-charset', 'accept-encoding', 'access-control-request-headers', 'access-control-request-method',
  'connection', 'content-length', 'cookie', 'cookie2', 'date', 'dnt', 'expect', 'host', 'keep-alive',
  'origin', 'referer', 'set-cookie', 'te', 'trailer', 'transfer-encoding', 'upgrade', 'user-agent', 'via',
];
export const FORBIDDEN_PREFIXES = ['proxy-', 'sec-'];
// Response headers no runner reports (connection-level, cookies): the same list natively
export const HIDDEN_RESPONSE_HEADERS = ['connection', 'keep-alive', 'proxy-connection', 'set-cookie', 'set-cookie2', 'te', 'trailer', 'transfer-encoding', 'upgrade'];
const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;

/** The host of an absolute URL, lowercased (no userinfo or port; IPv6 keeps its brackets), or null. */
export function urlHost(url) {
  const i = url.indexOf('://');
  if (i < 0) return null;
  let authority = url.slice(i + 3).split(/[/?#]/)[0];
  authority = authority.slice(authority.lastIndexOf('@') + 1);
  const host = authority.startsWith('[') ? authority.slice(0, authority.indexOf(']') + 1) : authority.split(':')[0];
  return host ? host.toLowerCase() : null;
}

/**
 * The player's answers for one run (the page asks: `ask(subject)` -> boolean or a
 * promise of it; subjects are `net:<host>`). Each subject is asked once per run;
 * pages remember "always" and "never" themselves.
 */
export class Consent {
  constructor(ask) { this.ask = ask; this.answers = new Map(); this.pending = new Map(); }
  /** true / false if answered, else a promise of the answer. */
  check(subject) {
    if (this.answers.has(subject)) return this.answers.get(subject);
    if (!this.pending.has(subject)) {
      this.pending.set(subject, Promise.resolve().then(() => this.ask(subject)).then((v) => !!v, () => false).then((v) => {
        this.answers.set(subject, v);
        this.pending.delete(subject);
        return v;
      }));
    }
    return this.pending.get(subject);
  }
}

/**
 * Which hosts guests may reach (gasm:net and gasm:fetch): `allowNet` false (none),
 * true (all) or a list of host names (`*.example.org`: its subdomains). With a
 * Consent, other hosts are the player's choice.
 */
export class NetPolicy {
  constructor(allowNet, consent = null) {
    this.allowed = !!allowNet;
    this.hosts = Array.isArray(allowNet) ? allowNet.map((h) => String(h).toLowerCase()) : [];
    this.consent = consent;
  }
  /** null: allowed; a string: refused (why); a promise: asking the player (resolves to null or why). */
  verdict(url) {
    const why = this.refusal(url);
    if (!why || !this.consent) return why;
    const host = urlHost(url) ?? '';
    const said = (ok) => (ok ? null : `the player said no to ${host}`);
    const a = this.consent.check(`net:${host}`);
    return typeof a === 'boolean' ? said(a) : a.then(said);
  }
  permits(host) {
    if (!this.allowed) return false;
    host = String(host ?? '').replace(/\.+$/, '').toLowerCase();
    return !this.hosts.length || this.hosts.some((p) => (p.startsWith('*.')
      ? host.length > p.length - 1 && host.endsWith(p.slice(1))
      : host === p));
  }
  /** Why a URL is refused (for the log), or null. */
  refusal(url) {
    if (!this.allowed) return 'networking not enabled';
    const host = urlHost(url) ?? '';
    return this.permits(host) ? null : `${host} is not an allowed host (${this.hosts.join(',')})`;
  }
}

/** Parse and check a request description: { method, url, headers: [[k, v]] }, or throws why it's refused. */
export function parseDesc(json, bodyLen) {
  let v;
  try { v = JSON.parse(json); } catch (e) { throw new Error(`invalid JSON: ${e.message}`); }
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('the description must be a JSON object');
  if (v.method !== undefined && typeof v.method !== 'string') throw new Error('method must be a string');
  const method = (v.method ?? 'GET').toUpperCase();
  if (!METHODS.includes(method)) throw new Error(`method ${method} is not supported`);
  if (typeof v.url !== 'string') throw new Error('url must be a string');
  const url = v.url, lower = url.toLowerCase();
  if (!(lower.startsWith('http://') || lower.startsWith('https://')) || !urlHost(url)) throw new Error(`not an absolute http(s) URL: ${url}`);
  if (/[\x00-\x20\x7f]/.test(url)) throw new Error(`invalid URL: ${url}`);
  const headers = [];
  if (v.headers !== undefined) {
    if (!v.headers || typeof v.headers !== 'object' || Array.isArray(v.headers)) throw new Error('headers must be an object');
    for (const [k, val] of Object.entries(v.headers)) {
      if (typeof val !== 'string') throw new Error(`header ${k}: the value must be a string`);
      if (!TOKEN.test(k)) throw new Error(`invalid header name ${JSON.stringify(k)}`);
      if (/[\r\n\0]/.test(val)) throw new Error(`invalid value for header ${k}`);
      const n = k.toLowerCase();
      if (FORBIDDEN_HEADERS.includes(n) || FORBIDDEN_PREFIXES.some((p) => n.startsWith(p))) throw new Error(`header ${k} can't be set (browsers refuse it)`);
      headers.push([n, val]);
    }
  }
  if (bodyLen > 0 && (method === 'GET' || method === 'HEAD')) throw new Error(`a ${method} request can't have a body`);
  if (bodyLen > MAX_REQUEST_BODY) throw new Error(`request body over ${MAX_REQUEST_BODY} bytes`);
  return { method, url, headers };
}

const ENC = new TextEncoder();
/** FNV-1a 64 of method, URL and body as 16 hex digits: the name of a recorded response (as natively). */
export function recordKey(method, url, body) {
  let h = 0xcbf29ce484222325n;
  const step = (b) => { h = ((h ^ BigInt(b)) * 0x100000001b3n) & 0xffffffffffffffffn; };
  for (const b of ENC.encode(method)) step(b);
  step(0);
  for (const b of ENC.encode(url)) step(b);
  step(0);
  for (const b of body) step(b);
  return h.toString(16).padStart(16, '0');
}

export class FetchRequests {
  /**
   * allowNet: false | true | host list. replay(key) -> { status, headers, body } | null:
   * answer from records only. record(key, { method, url, status, headers, body }): store
   * completed responses (live requests). userAgent: sent as User-Agent (Node; pages leave
   * it to the browser, where setting it would also need a CORS preflight).
   */
  constructor(allowNet, log, { replay = null, record = null, userAgent = null, consent = null } = {}) {
    this.userAgent = userAgent;
    this.policy = new NetPolicy(allowNet, consent);
    this.log = log;
    this.replay = replay;
    this.record = record;
    this.reqs = new Map();
    this.next = 1;
    this.denied = new Set();
  }

  /** The request, or null once closed; throws (traps) for a handle `request` never returned. */
  get(h) {
    if (!Number.isInteger(h) || h <= 0 || h >= this.next) throw new Error(`gasm:fetch: invalid handle ${h}`);
    return this.reqs.get(h) ?? null;
  }

  request(desc, body, frame) {
    let d;
    try { d = parseDesc(desc, body.length); } catch (e) { this.log(`[gasm] fetch: refused: ${e.message}`); return -1; }
    if (this.reqs.size >= MAX_REQUESTS) { this.log(`[gasm] fetch: too many requests (max ${MAX_REQUESTS}): ${d.url}`); return -1; }
    const r = { state: FETCH_PENDING, status: 0, headers: null, chunks: [], queued: 0, readyFrame: 0, abort: null, pull: null };
    if (this.replay) {
      r.readyFrame = frame + 1;
      const rec = this.replay(recordKey(d.method, d.url, body));
      if (rec) Object.assign(r, { state: FETCH_DONE, status: rec.status, headers: rec.headers, chunks: rec.body.length ? [rec.body] : [], queued: rec.body.length });
      else { this.log(`[gasm] fetch: ${d.method} ${d.url} is not recorded`); r.state = FETCH_FAILED; }
    } else {
      const why = this.policy.verdict(d.url);
      if (typeof why === 'string') {
        const host = urlHost(d.url);
        if (!this.denied.has(host)) { this.denied.add(host); this.log(`[gasm] fetch: denied ${d.url} (${why})`); }
        return -1;
      }
      if (typeof fetch !== 'function') { this.log('[gasm] fetch: no fetch() here'); return -1; }
      if (why) {   // the player is asked: pending until the answer
        why.then((no) => {
          if (r.closed) return;
          if (no) { this.log(`[gasm] fetch: denied ${d.url} (${no})`); Object.assign(r, { state: FETCH_FAILED }); } else this.start(r, d, body);
        });
      } else this.start(r, d, body);
    }
    const h = this.next++;
    this.reqs.set(h, r);
    return h;
  }

  start(r, d, body) {
    const abort = new AbortController();
    r.abort = abort;
    const timer = setTimeout(() => abort.abort(new Error('timed out')), TIMEOUT_MS);
    const fail = (e) => {
      if (r.state >= FETCH_DONE) return;
      if (!r.closed) this.log(`[gasm] fetch: ${d.method} ${d.url}: ${e?.message ?? abort.signal.reason?.message ?? e}`);
      Object.assign(r, { state: FETCH_FAILED, status: 0, chunks: [], queued: 0 });
      clearTimeout(timer);
    };
    const headers = this.userAgent ? [...d.headers, ['user-agent', this.userAgent]] : d.headers;
    const init = { method: d.method, headers, credentials: 'omit', cache: 'no-store', redirect: 'follow', signal: abort.signal };
    if (body.length) init.body = body;
    fetch(d.url, init).then(async (resp) => {
      const why = resp.url && resp.url !== d.url ? this.policy.refusal(resp.url) : null;
      if (why) throw new Error(`redirect to ${resp.url} denied (${why})`);
      let headers = '';
      // fetch()'s Headers are sorted by name, repeated names joined with ", " (as natively);
      // the body is decoded, so content-encoding goes, and content-length with it if it applied
      const enc = resp.headers.get('content-encoding');
      const drop = new Set([...HIDDEN_RESPONSE_HEADERS, 'content-encoding', ...(enc && enc.toLowerCase() !== 'identity' ? ['content-length'] : [])]);
      for (const [k, v] of resp.headers) if (!drop.has(k)) headers += `${k}: ${v.trim()}\n`;
      Object.assign(r, { state: FETCH_HEADERS, status: resp.status, headers });
      const all = this.record ? [] : null;
      let total = 0;
      if (resp.body) {
        const reader = resp.body.getReader();
        for (;;) {
          // a guest that stops reading pauses the download
          while (r.queued >= QUEUED && !abort.signal.aborted) await new Promise((res) => { r.pull = res; });
          if (abort.signal.aborted) return;
          const { done, value } = await reader.read();
          if (done) break;
          total += value.length;
          if (total > MAX_BODY) { abort.abort(); throw new Error(`body over ${MAX_BODY} bytes`); }
          r.chunks.push(value);
          r.queued += value.length;
          all?.push(value);
        }
      }
      r.state = FETCH_DONE;
      clearTimeout(timer);
      if (all) {
        const bytes = new Uint8Array(total);
        let o = 0;
        for (const c of all) { bytes.set(c, o); o += c.length; }
        this.record(recordKey(d.method, d.url, body), { method: d.method, url: d.url, status: resp.status, headers, body: bytes });
      }
    }).catch(fail);
  }

  /** [state, status, headers] as the guest may see them in `frame`. */
  view(h, frame) {
    const r = this.get(h);
    if (!r) return [FETCH_FAILED, 0, null];
    if (frame < r.readyFrame) return [FETCH_PENDING, 0, null];
    return [r.state, r.status, r.headers];
  }
  state(h, frame) { return this.view(h, frame)[0]; }
  status(h, frame) { return this.view(h, frame)[1]; }
  headers(h, frame) { return this.view(h, frame)[2]; }

  /** Up to `cap` body bytes (empty: none yet), or -1 once done and drained or failed. */
  read(h, cap, frame) {
    const r = this.get(h);
    if (!r) return -1;
    if (frame < r.readyFrame || r.state === FETCH_PENDING) return new Uint8Array(0);
    if (!r.chunks.length) return r.state >= FETCH_DONE ? -1 : new Uint8Array(0);
    const out = new Uint8Array(Math.min(cap, r.queued));
    let o = 0;
    while (o < out.length) {
      const c = r.chunks[0], n = Math.min(c.length, out.length - o);
      out.set(c.subarray(0, n), o);
      o += n;
      if (n === c.length) r.chunks.shift(); else r.chunks[0] = c.subarray(n);
    }
    r.queued -= out.length;
    if (r.pull && r.queued < QUEUED) { const p = r.pull; r.pull = null; p(); }
    return out;
  }

  close(h) {
    const r = this.get(h);
    if (!r) return;
    this.reqs.delete(h);
    r.closed = true;   // cancelled by the guest: not an error to log
    if (r.state < FETCH_DONE) r.abort?.abort();
    r.pull?.();
  }

  /** Cancel everything (the runner is shutting down). */
  closeAll() {
    for (const h of [...this.reqs.keys()]) this.close(h);
  }
}
