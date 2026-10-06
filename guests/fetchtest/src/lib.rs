//! Test guest for gasm:fetch. Sends a fixed set of requests to `base` (param; the
//! test server in scripts/fetch-server.mjs, or recorded responses with
//! `--fetch-replay tests/fixtures/fetch`), logs each result and folds statuses and
//! bodies into a hash it presents, so runners' video hashes differ if anything does.
//! The streamed body is read 4 KiB per frame. Ends (exit 0) when every request has
//! finished, logging "[fetchtest] done ...".

use gasm::fetch::{Request, Response, State};

// the hosts it reaches (the test servers), asked about once up front in a window
gasm::manifest!(r#"{ "manifest": 1, "name": "fetchtest", "requires": ["gasm:fetch"], "hosts": ["127.0.0.1", "localhost"] }"#);
use gasm::log;

fn fnv(h: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(h, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

struct Job {
    name: &'static str,
    resp: Option<Response>,
    body: Vec<u8>,
    /// read at most this many bytes per frame (0: all that arrived)
    per_frame: usize,
    finished: bool,
    /// what it got: folded into the total in job order (requests finish in any order)
    hash: u32,
}

struct Test {
    jobs: Vec<Job>,
    hash: u32,
    frame: u32,
    fb: Vec<u8>,
}

impl gasm::Game for Test {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        if !gasm::fetch::available() {
            return Err("no gasm:fetch".into());
        }
        let base = gasm::param("base").unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let mut hash = 0x811c_9dc5u32;
        // the runner must refuse these (same rules everywhere): -1, no handle
        for (name, req) in [
            ("cookie header", Request::get(&format!("{base}/hello")).header("cookie", "a=1")),
            ("GET with a body", Request::get(&format!("{base}/hello")).body(b"x".to_vec())),
            ("relative URL", Request::get("/hello")),
        ] {
            let refused = req.send().is_none();
            log!("[fetchtest] {name}: {}", if refused { "refused" } else { "ACCEPTED" });
            hash = fnv(hash, &[refused as u8]);
        }
        let job = |name, req: Request, per_frame| Job { name, resp: req.send(), body: Vec::new(), per_frame, finished: false, hash: 0 };
        let jobs = vec![
            job("get", Request::get(&format!("{base}/hello")), 0),
            job("post", Request::post(&format!("{base}/echo"), b"ping".to_vec()).header("x-test", "42").header("content-type", "text/plain"), 0),
            job("missing", Request::get(&format!("{base}/missing")), 0),
            job("redirect", Request::get(&format!("{base}/redirect")), 0),
            job("big", Request::get(&format!("{base}/big")), 4096),
            job("head", Request::new("HEAD", &format!("{base}/hello")), 0),
        ];
        for j in &jobs {
            if j.resp.is_none() {
                log!("[fetchtest] {}: not sent", j.name);
            }
        }
        Ok(Test { jobs, hash, frame: 0, fb: vec![0; 16 * 16 * 4] })
    }

    fn frame(&mut self) {
        self.frame += 1;
        let mut buf = vec![0u8; 64 * 1024];
        for j in self.jobs.iter_mut().filter(|j| !j.finished) {
            let Some(r) = j.resp.as_mut() else {
                j.finished = true;
                continue;
            };
            match r.state() {
                State::Pending => continue,
                State::Failed => {
                    log!("[fetchtest] {}: failed", j.name);
                    j.hash = fnv(fnv(0x811c_9dc5, j.name.as_bytes()), b"failed");
                    j.finished = true;
                    continue;
                }
                State::Headers | State::Done => {}
            }
            let cap = if j.per_frame > 0 { j.per_frame } else { buf.len() };
            loop {
                match r.read(&mut buf[..cap]) {
                    Some(0) => break,
                    Some(n) => {
                        j.body.extend_from_slice(&buf[..n]);
                        if j.per_frame > 0 {
                            break;
                        }
                    }
                    None => {
                        let status = r.status();
                        let ctype = r.header("content-type").unwrap_or_default();
                        let text = if j.body.len() <= 64 { String::from_utf8_lossy(&j.body).into_owned() } else { format!("{} bytes fnv={:08x}", j.body.len(), fnv(0x811c_9dc5, &j.body)) };
                        log!("[fetchtest] {}: {status} {ctype:?} {text:?}", j.name);
                        let headers: String = r.headers().unwrap_or_default().iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
                        j.hash = fnv(fnv(fnv(fnv(0x811c_9dc5, j.name.as_bytes()), &status.to_le_bytes()), headers.as_bytes()), &j.body);
                        j.finished = true;
                        break;
                    }
                }
            }
        }
        let total = self.jobs.iter().fold(self.hash, |h, j| fnv(h, &j.hash.to_le_bytes()));
        self.fb[..4].copy_from_slice(&total.to_le_bytes());
        gasm::present(&self.fb, 16, 16, 64);
        if self.jobs.iter().all(|j| j.finished) {
            self.hash = total;
            log!("[fetchtest] done: {} frames, hash={:08x}", self.frame, self.hash);
            gasm::exit(0);
        }
    }
}

gasm::game!(Test);
