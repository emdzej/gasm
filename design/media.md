# gasm:media: audio streams played by the runner

Status: **phase 1 implemented** (native: `gasm-run`, the Rust SDK; see the CHANGELOG);
phases 2 (browsers) and 3 (Godot) to come.

## Summary

A game plays internet radio or an on-demand track from a URL: the runner fetches,
decodes and plays the stream itself, mixed after the guest's own audio. The guest
opens a stream, starts and stops it, sets its volume and a low-pass filter, and polls
its state and metadata (the station's "now playing"), frame by frame, as with
`gasm:fetch`. Natively the runner decodes with symphonia; in the browser each stream
is an `<audio>` element.

Asked for by [Nowhere in Particular](https://github.com/emdzej/nowhereinparticular)
(issue #23 in its `docs/upstream/gasm-issues.md`): Mixtape, the in-game phone's radio.

## Why in the runner

- **No engine can do it in the guest.** Godot plays MP3 and Ogg only from whole
  buffers (no stream it has takes data while it plays); decoding into an
  `AudioStreamGenerator` is per-sample script work.
- **Browsers:** `gasm:fetch` needs CORS, which most stations don't send; an `<audio>`
  element plays them without it (the page never reads the samples).
- **Thousands of hosts:** stations live everywhere, so `--allow-net`'s host list
  doesn't fit; streams get a permission of their own (below), for audio only.

## The API

Module `gasm:media` (optional; probe `has("gasm:media")`). Handles are numbered in
creation order and never reused; at most 4 streams are open at once.

| function | signature | what |
|---|---|---|
| `open` | `(ptr, len) -> i32` | `{"url": "https://..."}`: a stream, not yet playing. A handle, or -1 when refused (no permission, a private address, too many open, a bad URL; `error(-1, ...)` says which). https only. |
| `play`, `pause` | `(h)` | Start (connects on the first play) and pause. A live stream paused for long reconnects at the next play. |
| `close` | `(h)` | Stop and forget it. |
| `set_volume` | `(h, gain: f32)` | 0 to 1 (more is clamped), applied smoothly. |
| `set_lowpass` | `(h, hz: f32)` | A low-pass filter at `hz` (music heard from outside a car); 0 is off. Only where `caps` says so. |
| `caps` | `(h) -> u32` | Flags: `VOLUME` (iOS Safari can't), `LOWPASS`, `METADATA` (natively all three; in a browser `LOWPASS` and `METADATA` only where the stream sends CORS). Known once the stream is playing. (No seeking: no `SEEKABLE`.) |
| `state` | `(h) -> i32` | `CONNECTING`, `PLAYING`, `PAUSED`, `FAILED`, `ENDED` (a track played to its end). |
| `error` | `(h, ptr, cap) -> i32` | Why it failed (or why `open` refused), as text; its length. |
| `position`, `duration` | `(h) -> f64` | Seconds, for tracks; `duration` is -1 for live streams. |
| `metadata` | `(h, ptr, cap) -> i32` | JSON `{"name", "title", "codec", "bitrate"}` (ICY: `icy-name`, `StreamTitle`); its length. |
| `metadata_version` | `(h) -> u32` | Changes whenever the metadata does (a new song), so a game polls cheaply. |

Errors and states are the same strings and numbers on every runner. A call on a closed
or unknown handle traps, as everywhere in gasm.

## Permissions

A stream is a request from the player's machine, and its URL can carry data out (a
query string), so the player decides, once per game:

- **The manifest declares it:** `{"media": true}`. The player is asked up front with
  the manifest's other questions ("Play internet radio from any site?"), and the answer
  is remembered like the others (`consent.rs`, `Consent` in `lib/fetch.js`; subject
  `media`). Without the manifest entry `open` returns -1.
- **`--allow-media`** is the command line's yes (any public host), or
  `--allow-media=a,b` for a list. Headless runs never ask: without the flag they refuse.
- **No private or loopback addresses** (the player's LAN, `localhost`): refused unless
  listed (`--allow-media=127.0.0.1`, NIP's smoke test), checked again on every
  redirect. Browsers enforce their own rules on top.
- **It doesn't widen `--allow-net`:** a media permission opens audio streams only.
- **Known gap:** natively the host is resolved and checked before connecting, but the
  HTTP client resolves it again, so a host whose DNS answer changes in between (DNS
  rebinding) could still reach a private address. Closing it means a resolver of our
  own for the client (ureq's is unversioned API); until then the check stops plain
  private URLs, not a determined rebinding server.

## Native runner

- A thread per stream: an HTTP GET (ureq, rustls, the runner's TLS), with
  `Icy-MetaData: 1`; ICY blocks are cut out of the audio and parsed (`StreamTitle`).
- Decoding with symphonia: MP3, AAC-LC (ADTS), Ogg Vorbis, FLAC. HE-AAC (AAC+) isn't
  supported (NIP leaves those stations out of its list).
- About 2 s buffered, resampled to the output rate (the runner's `Resampler`), mixed
  into the cpal output after the guest's `audio_push` samples, with volume ramps and
  a biquad low-pass.
- A live stream that drops reconnects (backoff, a few tries), then `FAILED`.
- The window runner mixes and plays; headless runs decode and update state and
  metadata but output nothing.

## Browser runner

- An `HTMLAudioElement` per stream, on the page (Worker mode posts the calls to the
  main thread and the state back with each batch).
- First `crossOrigin = "anonymous"`: with CORS the element goes through Web Audio
  (`MediaElementAudioSourceNode`, a `BiquadFilterNode`), so `LOWPASS` works and the
  metadata can be read. Without CORS that load fails, so it retries without
  `crossOrigin`: element volume only, no metadata (Web Audio on a cross-origin
  stream without CORS plays silence).
- iOS Safari ignores element volume: `caps` says so.
- Playing needs the page to have had a click or key, which the player always has
  (its start button).
- The host gets an option for embedders (`media: { allow, ask }`); `gasm-headless`
  refuses without `--allow-media`.

## Reproducible runs

Stream audio is mixed after the guest's audio, so it is never hashed. But connecting
takes real time: a game that draws a stream's state draws it at different frames, so
headless runs with media are not reproducible, by design, and refuse media unless
`--allow-media` is given. A recorded stream (as `gasm:fetch` replays responses) could
come later if a test needs reproducibility.

## Guests

- Rust SDK: `gasm::media` (`Stream::open(url)`, `play`, `state`, ...), a closing `Drop`.
- C: `gasm.h` from `abi.json`, as usual.
- Godot: `media_*` on the `Gasm` singleton (`platform/gasm/api`), which NIP's
  `Platform` wraps.

## Tests

- A local stream server (in `scripts/`, like `fetch-server.mjs`): an MP3 with ICY
  metadata that changes, a track that ends, a stream that drops.
- Native, Node and Chrome: the same refusals (no permission, a private address
  unlisted, a fifth stream), the same state progression and metadata from
  `127.0.0.1`. The player in Chrome: CORS and non-CORS streams give the `caps`
  above.
- NIP's smoke test: the state reaches `PLAYING` and metadata arrives from a local
  server.

## Licences

symphonia is MPL-2.0 (file-level copyleft: fine for gasm-run, with its notice in
`THIRD-PARTY.txt`). Each decoder crate gets checked before it is added.

## Phases

1. **Native:** the ABI (`abi.json` and the generated files), the stream threads,
   decoding, mixing, the filter, ICY, reconnects; the permission (manifest, consent,
   `--allow-media`, private addresses); the Rust SDK; the local stream server and
   the native tests.
2. **Browser:** the JS host, Worker mode, the player's question; the same tests in
   Node and Chrome.
3. **Godot:** the `Gasm` singleton's calls; the CHANGELOG, the site's docs.

## Not in scope

HLS and DASH (later, if at all), recording, seeking a live stream, plain http
(browsers block it on https pages; NIP's list is https only), and reading the samples
(a guest never gets the stream's audio: that would also need CORS in a browser).
