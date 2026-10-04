# gasm:fetch: HTTP requests for guests

Status: **implemented** (both runners, the Rust SDK, Godot's `HTTPClient`; see the CHANGELOG).

## Summary

Games call web APIs: weather, leaderboards, map tiles, content updates. gasm
has `gasm:net`, which is WebSocket messages only, and guests have no sockets, so
a game can't make an ordinary HTTP request. `gasm:fetch` is an optional,
permissioned HTTP client in the runner: the guest describes a request, the
runner performs it (TLS included) and the guest polls for the response, frame by
frame, like `gasm:net`. Natively it is a background thread per request; in the
browser it is `fetch()`.

Asked for by [Nowhere in Particular](https://github.com/emdzej/nowhereinparticular)
("sync with the real world": MET Norway's forecast API).

## Why not sockets

- Browsers can't open TCP sockets: an HTTP client in the guest (Godot's
  `HTTPClientTCP`, libcurl) can't work there at all.
- TLS in the guest means a crypto library and a CA store in every game.
  Runners already have both (rustls natively, the browser's own).
- One request/response API is easy to permission per host and to replay in
  tests; a socket isn't.

## The API

Module `gasm:fetch` (optional; probe `has("gasm:fetch")`):

| Import | Signature | |
|---|---|---|
| `request` | `(desc_ptr, desc_len, body_ptr, body_len) -> i32` | Start a request. `desc` is JSON: `{"method":"GET","url":"https://…","headers":{"accept":"application/json"}}` (`method` defaults to `GET`, `headers` to none). Returns a handle > 0, or `-1` if denied (not allowed, or the host isn't on the list), invalid, or too many are open (16). |
| `state` | `(h) -> u32` | `0` pending, `1` the response's status and headers are in, `2` done (the whole body has arrived), `3` failed (network error, timeout, too large, refused by the browser). A closed handle reports `3`; one `request` never returned traps (as in `gasm:net`). |
| `status` | `(h) -> i32` | The HTTP status (`200`, `404`), or `0` before the headers or after a failure. Redirects are followed: this is the final response's. |
| `headers` | `(h, dst, cap) -> i32` | Response headers as `name: value\n` lines, names lowercased, in the order received (browsers: only those CORS exposes). Length (copied only if ≤ `cap`; `cap = 0` queries), `-1` before state 1. |
| `read` | `(h, dst, cap) -> i32` | Body bytes that have arrived: copies up to `cap` and returns the count; `0` if none are waiting yet; `-1` once the body is done and drained, or after a failure. |
| `close` | `(h)` | Cancel if still running and free the handle. Closing again does nothing. |

Design choices:

- **JSON for the description, bytes for the body**: as `gasm:gfx` creation
  calls. Headers are a map of strings; a body is raw bytes (`body_len = 0`: none).
- **States, not callbacks**: the guest polls once per frame. Nothing calls into
  the guest from outside a frame, so determinism tooling and stack switching
  are unaffected.
- **The body streams**: `read` hands out what has arrived, so a large download
  doesn't need one buffer of its size, and progress is visible.
- **Limits**: 16 open requests; a response body of at most 64 MiB (larger
  fails); 30 s to connect, 120 s overall. Request bodies at most 16 MiB.
- **Methods** `GET HEAD POST PUT PATCH DELETE OPTIONS`; schemes `http`, `https`.
  Request headers the browser forbids (`host`, `cookie`, `content-length`, …) are
  refused the same way on every runner, so a game can't depend on them: the
  request returns `-1`.
- **No cookies, no credentials, no cache control by the runner**: requests are
  stateless (`credentials: 'omit'` in browsers). Gzip and deflate are decoded
  on every runner (browsers always do).

## Permissions

A request reaches the network only if the player allowed it:

- **Native**: off unless `--allow-net` (everything) or
  `--allow-net=api.met.no,tiles.example.org` (only those hosts, exact names, or
  `*.example.org` for subdomains). The same flag and list apply to `gasm:net`.
  Redirects to a host not on the list fail.
- **Browser**: the page decides (`allowNet` option of `GasmHost`, a list of hosts
  or `true`), and the browser's rules apply on top: CORS (the API must send
  `Access-Control-Allow-Origin`), mixed content (an https page can't fetch http).
- **Headless**: off by default, like `gasm:net`.

A denied request is logged once per host, so players see what a game tried.

## Reproducible runs

Responses arrive whenever the network delivers them, so a game using
`gasm:fetch` isn't reproducible by itself. Headless runs can record and replay:

- `--fetch-record DIR`: perform requests for real (needs `--allow-net`) and store
  each response in `DIR` (`<key>.json`: status, headers; `<key>.body`), keyed by
  FNV-1a 64 of method, URL and request body.
- `--fetch-replay DIR`: never touch the network. A recorded request becomes state
  1 and 2 at the start of the next frame, with the whole body readable; an
  unrecorded one fails (state 3) at the next frame. Same frames on every runner,
  so hashes compare (the determinism suite does).

Both headless runners have both flags.

## Guests

- **C**: the generated `gasm_fetch_*` imports in `gasm.h`.
- **Rust**: `gasm::fetch::Request::get(url)` / `::new(method, url)`, `.header()`,
  `.body()`, `.send()` → `Response` with `state()`, `status()`, `headers()`,
  `read(&mut buf)`, `read_to_end()`; dropping it closes the handle.
- **Godot**: `HTTPClient` has a gasm backend (`platform/gasm/http_client_gasm.cpp`),
  so `HTTPRequest` works unchanged, `https://` included (the runner does TLS;
  Godot's `TLSOptions` are ignored). `HTTPClient` connects to a host, then
  sends requests; the backend keeps the host and makes one `gasm:fetch`
  request per `request()`.

## Not in scope

- Streaming request bodies, WebTransport, server-sent events (use `gasm:net`).
- Caching. A game that wants it stores responses in `gasm:storage`.
- Proxies beyond what the platform does (browsers: the system's; native: none).
