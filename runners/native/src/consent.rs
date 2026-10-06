//! Player consent: the window runner asks before a game reaches outside itself, a
//! network host (gasm:net, gasm:fetch) or saving files for the player (gasm:files).
//! Four answers: this time (until the game ends), always (remembered), no (asked
//! again next run) and never (remembered). Remembered answers live in
//! `<data dir>/gasm/consent/<game>.txt`, one `allow <subject>` / `deny <subject>`
//! line each; `gasm-run --forget-consent <game|all>` removes them.
//!
//! Requests wait while a question is open: a connection stays connecting, a fetch
//! or a save pending, and each fails as a refused one would if the answer is no.
//! Headless runs never ask (they deny what the command line didn't allow).

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// What a game asks for: `net:<host>` (`net:a.org,b.org` for a manifest's hosts,
/// asked at once and answered for each), `save`, or `mod:<file>` (a mod whose
/// manifest asks for hosts; allowing it allows them).
pub type Subject = String;

pub fn net_subject(host: &str) -> Subject {
    format!("net:{host}")
}
pub const SAVE: &str = "save";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// allowed until the game ends
    ThisTime,
    /// allowed, remembered
    Always,
    /// denied this time, asked again next run
    No,
    /// denied, remembered
    Never,
}

#[derive(Debug)]
pub struct Store {
    file: Option<PathBuf>,
    remembered: BTreeMap<Subject, bool>,
    session: BTreeMap<Subject, bool>,
    /// asked for and not answered yet, in order
    questions: VecDeque<Subject>,
    /// headless and tests: unanswered means no
    ask: bool,
    /// mod file -> the hosts its manifest asks for
    mod_hosts: BTreeMap<String, Vec<String>>,
}

/// Shared by the host's gasm:net, gasm:fetch and gasm:files and the window runner.
pub type Consent = Arc<Mutex<Store>>;

impl Store {
    /// A store that asks (the window runner), with this game's remembered answers.
    pub fn open(game: &str) -> Store {
        let file = crate::storage::data_root().map(|r| r.join("consent").join(format!("{game}.txt")));
        let mut remembered = BTreeMap::new();
        if let Some(text) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
            for line in text.lines() {
                match line.trim().split_once(' ') {
                    Some(("allow", s)) => _ = remembered.insert(s.to_owned(), true),
                    Some(("deny", s)) => _ = remembered.insert(s.to_owned(), false),
                    _ => {}
                }
            }
        }
        Store { file, remembered, session: BTreeMap::new(), questions: VecDeque::new(), ask: true, mod_hosts: BTreeMap::new() }
    }

    /// A store that never asks: unanswered is no (headless runs).
    pub fn never_ask() -> Store {
        Store { file: None, remembered: BTreeMap::new(), session: BTreeMap::new(), questions: VecDeque::new(), ask: false, mod_hosts: BTreeMap::new() }
    }

    pub fn shared(self) -> Consent {
        Arc::new(Mutex::new(self))
    }

    /// The answer so far: Some(allowed), or None while the player hasn't answered
    /// (the question is queued once).
    pub fn check(&mut self, subject: &str) -> Option<bool> {
        if let Some(&a) = self.session.get(subject).or_else(|| self.remembered.get(subject)) {
            return Some(a);
        }
        if !self.ask {
            return Some(false);
        }
        if !self.questions.iter().any(|q| q == subject) {
            self.questions.push_back(subject.to_owned());
        }
        None
    }

    /// A manifest's hosts and saves, asked about before the game starts (one question
    /// for the hosts not answered yet).
    pub fn ask_up_front(&mut self, hosts: &[String], files: bool) {
        let open: Vec<&str> = hosts.iter().map(String::as_str).filter(|h| self.answered(&net_subject(h)).is_none()).collect();
        if !open.is_empty() {
            self.queue(net_subject(&open.join(",")));
        }
        if files {
            _ = self.check(SAVE);
        }
    }

    /// A mod whose manifest asks for hosts: Some(allowed) if answered, else asked
    /// (before the game starts) and None.
    pub fn check_mod(&mut self, file: &str, hosts: &[String]) -> Option<bool> {
        self.mod_hosts.insert(file.to_owned(), hosts.to_vec());
        let subject = format!("mod:{file}");
        let a = self.answered(&subject);
        if a == Some(true) {
            for h in hosts {
                self.session.insert(net_subject(h), true);
            }
        }
        if a.is_none() {
            if !self.ask {
                return Some(false);
            }
            self.queue(subject);
        }
        a
    }

    fn answered(&self, subject: &str) -> Option<bool> {
        self.session.get(subject).or_else(|| self.remembered.get(subject)).copied()
    }

    fn queue(&mut self, subject: Subject) {
        if self.ask && !self.questions.contains(&subject) {
            self.questions.push_back(subject);
        }
    }

    /// Whether questions are waiting (the window runner doesn't start a game meanwhile).
    pub fn asking(&self) -> bool {
        !self.questions.is_empty()
    }

    /// The hosts a mod's manifest asks for (for the question).
    pub fn mod_hosts(&self, file: &str) -> &[String] {
        self.mod_hosts.get(file).map_or(&[], Vec::as_slice)
    }

    /// The next question to put to the player.
    pub fn question(&self) -> Option<&Subject> {
        self.questions.front()
    }

    pub fn answer(&mut self, subject: &str, answer: Answer) {
        self.questions.retain(|q| q != subject);
        let allowed = matches!(answer, Answer::ThisTime | Answer::Always);
        // a manifest's hosts: an answer for each; an allowed mod allows its hosts
        let mut subjects: Vec<String> = match subject.strip_prefix("net:") {
            Some(hosts) => hosts.split(',').map(net_subject).collect(),
            None => vec![subject.to_owned()],
        };
        if let (Some(file), true) = (subject.strip_prefix("mod:"), allowed) {
            for h in self.mod_hosts(file).to_vec() {
                self.session.insert(net_subject(&h), true);
            }
        }
        for s in subjects.drain(..) {
            self.session.insert(s.clone(), allowed);
            if matches!(answer, Answer::Always | Answer::Never) {
                self.remembered.insert(s, allowed);
            }
        }
        if matches!(answer, Answer::Always | Answer::Never) {
            self.save();
        }
        eprintln!("[gasm] consent: {subject}: {}", match answer {
            Answer::ThisTime => "allowed this time",
            Answer::Always => "always allowed",
            Answer::No => "denied this time",
            Answer::Never => "never allowed",
        });
    }

    fn save(&self) {
        let Some(f) = &self.file else { return };
        let text: String = self.remembered.iter().map(|(s, &a)| format!("{} {s}\n", if a { "allow" } else { "deny" })).collect();
        let r = f.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(f, text));
        if let Err(e) = r {
            eprintln!("[gasm] consent: {}: {e}", f.display());
        }
    }
}

/// `--forget-consent <game|all>`: remove remembered answers; what was removed.
pub fn forget(game: &str) -> Result<String, String> {
    let dir = crate::storage::data_root().map(|r| r.join("consent")).ok_or("no data directory")?;
    if game == "all" {
        return match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(format!("removed every remembered answer ({})", dir.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("no remembered answers".into()),
            Err(e) => Err(format!("{}: {e}", dir.display())),
        };
    }
    let f = dir.join(format!("{game}.txt"));
    match std::fs::remove_file(&f) {
        Ok(()) => Ok(format!("removed the remembered answers for {game} ({})", f.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(format!("no remembered answers for {game}")),
        Err(e) => Err(format!("{}: {e}", f.display())),
    }
}

/// The question for the player, in words: what the game wants, and to what.
pub fn describe(subject: &str) -> (&'static str, String) {
    if let Some(hosts) = subject.strip_prefix("net:") {
        return ("wants to connect to", hosts.replace(',', ", "));
    }
    if let Some(file) = subject.strip_prefix("mod:") {
        return ("wants to load a mod that connects out", file.to_owned());
    }
    ("wants to save files for you", "in Pictures or Downloads".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        let mut s = Store { file: None, remembered: BTreeMap::new(), session: BTreeMap::new(), questions: VecDeque::new(), ask: true, mod_hosts: BTreeMap::new() };
        assert_eq!(s.check("net:a.org"), None);
        assert_eq!(s.check("net:a.org"), None);
        assert_eq!(s.check("save"), None);
        assert_eq!(s.question().map(String::as_str), Some("net:a.org")); // asked once each, in order
        s.answer("net:a.org", Answer::ThisTime);
        assert_eq!(s.check("net:a.org"), Some(true));
        assert_eq!(s.question().map(String::as_str), Some("save"));
        s.answer("save", Answer::Never);
        assert_eq!((s.check("save"), s.remembered.get("save")), (Some(false), Some(&false)));
        assert_eq!(s.question(), None);
        assert_eq!(Store::never_ask().check("net:b.org"), Some(false));
    }

    #[test]
    fn up_front_and_mods() {
        let mut s = Store::never_ask();
        s.ask = true;
        s.ask_up_front(&["a.org".into(), "b.org".into()], false);
        assert_eq!(s.question().map(String::as_str), Some("net:a.org,b.org"));
        s.answer("net:a.org,b.org", Answer::ThisTime);
        assert_eq!((s.check("net:a.org"), s.check("net:b.org")), (Some(true), Some(true)));
        assert_eq!(s.check_mod("roads.pck", &["c.org".into()]), None);
        assert_eq!(s.question().map(String::as_str), Some("mod:roads.pck"));
        s.answer("mod:roads.pck", Answer::ThisTime);
        assert_eq!((s.check_mod("roads.pck", &["c.org".into()]), s.check("net:c.org")), (Some(true), Some(true)));
        assert_eq!(Store::never_ask().check_mod("x.pck", &["d.org".into()]), Some(false));
    }
}
