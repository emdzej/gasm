//! gasm:files: files a game hands the player. `save` queues a copy; the runner writes
//! the queue after the frame (`Files::flush`) where the player finds it: images in
//! Pictures/<game>/, everything else in Downloads/<game>/, or `--save-dir`. Names are
//! reduced to a safe file name and never overwrite a file ("photo (2).png"). The game
//! never learns the path. Headless runs without `--save-dir` write nothing (each save
//! still completes at the end of its frame, so runs are reproducible).

use std::path::{Path, PathBuf};

/// The largest file a game can save
pub const MAX_SAVE: usize = 256 << 20;
/// Saves queued and not yet written, at most
pub const MAX_PENDING: usize = 16;

pub const PENDING: i32 = 0;
pub const SAVED: i32 = 1;
pub const FAILED: i32 = 2;

/// Where saves go.
#[derive(Clone, Debug)]
pub enum SaveTarget {
    /// `--no-save`: every save is refused
    Off,
    /// accepted and dropped (headless runs without `--save-dir`)
    Discard,
    /// Pictures/<game>/ for images, Downloads/<game>/ otherwise
    Defaults { game: String },
    /// `--save-dir`
    Dir(PathBuf),
}

pub struct Files {
    target: SaveTarget,
    /// the window runner asks the player before the first save to the default folders
    consent: Option<crate::consent::Consent>,
    /// state by handle - 1
    states: Vec<i32>,
    queue: Vec<(usize, String, String, Vec<u8>)>,
}

impl Default for Files {
    fn default() -> Files {
        Files::new(SaveTarget::Discard)
    }
}

impl Files {
    pub fn new(target: SaveTarget) -> Files {
        Files { target, consent: None, states: Vec::new(), queue: Vec::new() }
    }

    pub fn with_consent(mut self, consent: Option<crate::consent::Consent>) -> Files {
        self.consent = consent;
        self
    }

    /// Queue a save: a handle > 0, or -1 (logged).
    pub fn save(&mut self, name: &str, mime: &str, data: &[u8]) -> i32 {
        let why = match () {
            _ if matches!(self.target, SaveTarget::Off) => Some("saving is turned off (--no-save)"),
            _ if name.is_empty() || name.len() > 255 => Some("the name must be 1 to 255 bytes"),
            _ if !valid_mime(mime) => Some("the type must be type/subtype (image/png)"),
            _ if data.len() > MAX_SAVE => Some("over 256 MiB"),
            _ if self.queue.len() >= MAX_PENDING => Some("16 saves are still pending"),
            _ => None,
        };
        if let Some(why) = why {
            eprintln!("[gasm] files: save {name:?} refused: {why}");
            return -1;
        }
        self.states.push(PENDING);
        self.queue.push((self.states.len() - 1, name.to_owned(), mime.to_owned(), data.to_vec()));
        self.states.len() as i32
    }

    /// The state of a save, or None for a handle `save` never returned.
    pub fn state(&self, handle: i32) -> Option<i32> {
        usize::try_from(handle).ok().and_then(|h| h.checked_sub(1)).and_then(|i| self.states.get(i).copied())
    }

    /// Write what the game saved during the frame (between frames). Saves to the
    /// default folders wait (pending) until the player has answered.
    pub fn flush(&mut self) {
        if let (SaveTarget::Defaults { .. }, Some(c), false) = (&self.target, &self.consent, self.queue.is_empty()) {
            match c.lock().unwrap().check(crate::consent::SAVE) {
                None => return,
                Some(true) => {}
                Some(false) => {
                    for (i, name, ..) in std::mem::take(&mut self.queue) {
                        eprintln!("[gasm] files: save {name:?} refused: the player said no");
                        self.states[i] = FAILED;
                    }
                    return;
                }
            }
        }
        for (i, name, mime, data) in std::mem::take(&mut self.queue) {
            let dir = match &self.target {
                SaveTarget::Off => None,
                SaveTarget::Discard => {
                    self.states[i] = SAVED;
                    continue;
                }
                SaveTarget::Dir(d) => Some(d.clone()),
                SaveTarget::Defaults { game } => {
                    let base = if mime.starts_with("image/") { pictures_dir() } else { downloads_dir() };
                    base.map(|b| b.join(safe_name(game)))
                }
            };
            self.states[i] = match dir.ok_or_else(|| "no folder to save in".to_string()).and_then(|d| write_unique(&d, &safe_name(&name), &data)) {
                Ok(path) => {
                    eprintln!("[gasm] files: saved {}", path.display());
                    SAVED
                }
                Err(e) => {
                    eprintln!("[gasm] files: save {name:?} failed: {e}");
                    FAILED
                }
            };
        }
    }
}

/// `type/subtype` with RFC 6838 name characters.
pub fn valid_mime(m: &str) -> bool {
    let part = |s: &str| !s.is_empty() && s.len() <= 127 && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&c));
    m.split_once('/').is_some_and(|(t, s)| part(t) && part(s))
}

/// The last path component, with characters no file system takes replaced by `_`,
/// without leading dots or trailing dots and spaces, and not a Windows device name.
pub fn safe_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut s: String = last.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    s = s.trim_start_matches(['.', ' ']).trim_end_matches(['.', ' ']).to_owned();
    if s.is_empty() {
        return "file".into();
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
    if device { format!("_{s}") } else { s }
}

/// Create `dir/name`, or `dir/stem (2).ext` and so on if it exists.
fn write_unique(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..1000 {
        let path = dir.join(if n == 1 { name.to_owned() } else { format!("{stem} ({n}){ext}") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                return f.write_all(data).and_then(|_| f.sync_all()).map(|_| path.clone()).map_err(|e| {
                    let _ = std::fs::remove_file(&path);
                    format!("{}: {e}", path.display())
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Err(format!("{}: too many files named {name}", dir.display()))
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// An XDG user directory (`XDG_PICTURES_DIR="$HOME/Pictures"` in user-dirs.dirs).
fn xdg_user_dir(key: &str) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".config")))?;
    let text = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    let value = text.lines().find_map(|l| l.trim().strip_prefix(key)?.strip_prefix('='))?.trim().trim_matches('"');
    match value.strip_prefix("$HOME") {
        Some(rest) => home().map(|h| h.join(rest.trim_start_matches('/'))),
        None => Some(PathBuf::from(value)),
    }
}

pub fn pictures_dir() -> Option<PathBuf> {
    (!cfg!(any(target_os = "macos", windows))).then(|| xdg_user_dir("XDG_PICTURES_DIR")).flatten().or_else(|| home().map(|h| h.join("Pictures")))
}

pub fn downloads_dir() -> Option<PathBuf> {
    (!cfg!(any(target_os = "macos", windows))).then(|| xdg_user_dir("XDG_DOWNLOAD_DIR")).flatten().or_else(|| home().map(|h| h.join("Downloads")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("photo.png"), "photo.png");
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Users\\x\\shot.png"), "shot.png");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(safe_name("con.txt"), "_con.txt");
        assert_eq!(safe_name("..."), "file");
        assert_eq!(safe_name("map. "), "map");
    }

    #[test]
    fn mime_types() {
        assert!(valid_mime("image/png") && valid_mime("application/vnd.gasm+json"));
        assert!(!valid_mime("png") && !valid_mime("image/") && !valid_mime("text/plain; charset=utf-8"));
    }

    #[test]
    fn saves_are_written_once_each_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("gasm-files-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut f = Files::new(SaveTarget::Dir(dir.clone()));
        let a = f.save("shot.png", "image/png", b"one");
        let b = f.save("shot.png", "image/png", b"two");
        assert_eq!((f.state(a), f.state(b), f.state(3), f.state(0)), (Some(PENDING), Some(PENDING), None, None));
        f.flush();
        assert_eq!((f.state(a), f.state(b)), (Some(SAVED), Some(SAVED)));
        assert_eq!(std::fs::read(dir.join("shot.png")).unwrap(), b"one");
        assert_eq!(std::fs::read(dir.join("shot (2).png")).unwrap(), b"two");
        assert_eq!(Files::new(SaveTarget::Off).save("x.txt", "text/plain", b"x"), -1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
