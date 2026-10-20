//! Test guest for gasm:media. Opens the stream at param `url` and plays it, logging
//! every change: "[mediatest] state Playing", "[mediatest] title Song 2", a refusal
//! ("[mediatest] refused: ..."), a failure's error. Params: `pause` (a frame to pause
//! at) and `resume` (to play again), `volume` and `lowpass` (applied at once).
//! Its manifest declares "media": true (the player's question in a window; headless
//! runs need --allow-media).

use gasm::log;
use gasm::media::{State, Stream};

gasm::manifest!(r#"{ "manifest": 1, "name": "mediatest", "requires": ["gasm:media"], "media": true }"#);

struct Test {
    stream: Option<Stream>,
    frame: u32,
    state: Option<State>,
    version: u32,
    pause: Option<u32>,
    resume: Option<u32>,
}

fn num(name: &str) -> Option<f64> {
    gasm::param(name).and_then(|v| v.parse().ok())
}

impl gasm::Game for Test {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let url = gasm::param("url").unwrap_or_default();
        let stream = Stream::open(&url);
        match &stream {
            Some(s) => {
                log!("[mediatest] opened {url}");
                if let Some(v) = num("volume") {
                    s.set_volume(v as f32);
                }
                if let Some(hz) = num("lowpass") {
                    s.set_lowpass(hz as f32);
                }
                s.play();
            }
            None => log!("[mediatest] refused: {}", gasm::media::refusal()),
        }
        Ok(Test { stream, frame: 0, state: None, version: 0, pause: num("pause").map(|f| f as u32), resume: num("resume").map(|f| f as u32) })
    }

    fn frame(&mut self) {
        self.frame += 1;
        let Some(s) = &self.stream else { return };
        if Some(self.frame) == self.pause {
            s.pause();
        }
        if Some(self.frame) == self.resume {
            s.play();
        }
        let state = s.state();
        if self.state != Some(state) {
            self.state = Some(state);
            match state {
                State::Failed => log!("[mediatest] state Failed: {}", s.error()),
                State::Playing => {
                    let c = s.caps();
                    log!("[mediatest] state Playing (volume {}, lowpass {}, metadata {})", c.volume, c.lowpass, c.metadata);
                }
                State::Ended => log!("[mediatest] state Ended at {:.1} s", s.position()),
                other => log!("[mediatest] state {other:?}"),
            }
        }
        let v = s.metadata_version();
        if v != self.version {
            self.version = v;
            let m = s.metadata();
            log!("[mediatest] metadata name={:?} title={:?} codec={:?} bitrate={:?}", m.name, m.title, m.codec, m.bitrate);
        }
        if self.frame % 120 == 0 {
            log!("[mediatest] frame {} position {:.1} s duration {:?}", self.frame, s.position(), s.duration());
        }
    }
}

gasm::game!(Test);
