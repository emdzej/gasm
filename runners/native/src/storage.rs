//! gasm:storage for the native runner: one directory per game, one file per key.
//!
//! Windowed runs persist under the OS data directory (`--storage-dir` overrides);
//! headless runs use an in-memory store unless a directory is given, so tests
//! stay reproducible. A write goes to a temp file (named with `~`, which keys
//! can't contain), is synced, then renamed into place: once `set` returns, the
//! value survives a crash.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

pub const MAX_KEY: usize = 128;
pub const MAX_VALUE: usize = 1 << 20;
pub const QUOTA: usize = 16 << 20;

/// Why `set` failed; `code()` is the GASM_STORAGE_ERR_* value returned to the guest.
#[derive(Debug)]
pub enum StorageError {
    Key(String),
    Size(usize),
    Quota,
    Io(String),
}

impl StorageError {
    pub fn code(&self) -> i32 {
        match self {
            StorageError::Key(_) => -1,
            StorageError::Size(_) => -2,
            StorageError::Quota => -3,
            StorageError::Io(_) => -4,
        }
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Key(k) => write!(f, "invalid key {k:?}"),
            StorageError::Size(n) => write!(f, "value is {n} bytes (max {MAX_VALUE})"),
            StorageError::Quota => write!(f, "storage quota of {QUOTA} bytes exceeded"),
            StorageError::Io(e) => write!(f, "{e}"),
        }
    }
}

pub struct Storage {
    dir: Option<PathBuf>,
    /// sorted by key (keys are ASCII, so byte order = JS sort order)
    values: BTreeMap<String, Vec<u8>>,
    /// bytes counted against the quota (keys + values)
    used: usize,
    /// keys in order, rebuilt after a change (count/key enumerate it)
    keys: Option<Vec<String>>,
}

pub fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= MAX_KEY
        && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && k != "."
        && k != ".."
}

/// Default location: <data dir>/gasm/<game id>, e.g.
/// ~/Library/Application Support/gasm/sumo on macOS.
pub fn default_dir(game_id: &str) -> Option<PathBuf> {
    data_root().map(|r| r.join(game_id))
}

/// `<data dir>/gasm`: saves live in subdirectories, `keymap.txt` next to them.
pub fn data_root() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("gasm"))
}

fn temp_name(key: &str) -> String {
    format!(".{key}~tmp")
}

impl Storage {
    pub fn memory() -> Storage {
        Storage { dir: None, values: BTreeMap::new(), used: 0, keys: None }
    }

    /// Open (creating if needed) a directory-backed store and load its values.
    /// Temp files left by an interrupted write are removed.
    pub fn open(dir: PathBuf) -> Result<Storage, String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut s = Storage { dir: Some(dir.clone()), ..Storage::memory() };
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with("~tmp") {
                let _ = std::fs::remove_file(entry.path());
            } else if valid_key(&name) && entry.path().is_file() {
                if let Ok(v) = std::fs::read(entry.path()) {
                    s.used += name.len() + v.len();
                    s.values.insert(name, v);
                }
            }
        }
        Ok(s)
    }

    pub fn location(&self) -> String {
        self.dir.as_ref().map_or("memory".into(), |d| d.display().to_string())
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.values.get(key).map(Vec::as_slice)
    }

    pub fn set(&mut self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        if !valid_key(key) {
            return Err(StorageError::Key(key.to_owned()));
        }
        if value.len() > MAX_VALUE {
            return Err(StorageError::Size(value.len()));
        }
        let old = self.values.get(key).map_or(0, |v| key.len() + v.len());
        let used = self.used - old + key.len() + value.len();
        if used > QUOTA {
            return Err(StorageError::Quota);
        }
        if let Some(dir) = &self.dir {
            let tmp = dir.join(temp_name(key));
            let write = || -> std::io::Result<()> {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(value)?;
                f.sync_all()?;
                std::fs::rename(&tmp, dir.join(key))
            };
            if let Err(e) = write() {
                let _ = std::fs::remove_file(&tmp);
                return Err(StorageError::Io(e.to_string()));
            }
        }
        if !self.values.contains_key(key) {
            self.keys = None;
        }
        self.values.insert(key.to_owned(), value.to_vec());
        self.used = used;
        Ok(())
    }

    /// All keys, sorted.
    pub fn keys(&mut self) -> &[String] {
        self.keys.get_or_insert_with(|| self.values.keys().cloned().collect())
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let Some(v) = self.values.remove(key) else { return false };
        self.used -= key.len() + v.len();
        self.keys = None;
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(key));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_files_never_collide_with_keys() {
        let d = std::env::temp_dir().join(format!("gasm-storage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut s = Storage::open(d.clone()).unwrap();
        s.set(".foo.tmp", b"guest's own key").unwrap();
        s.set("foo", b"value").unwrap();
        assert_eq!(s.get(".foo.tmp"), Some(&b"guest's own key"[..]));
        std::fs::write(d.join(temp_name("bar")), b"left by a crash").unwrap();
        let mut s = Storage::open(d.clone()).unwrap();
        assert_eq!(s.keys(), [".foo.tmp", "foo"]);
        assert!(!d.join(temp_name("bar")).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn error_codes_and_quota_accounting() {
        let mut s = Storage::memory();
        assert_eq!(s.set("bad/key", b"x").unwrap_err().code(), -1);
        assert_eq!(s.set("big", &vec![0; MAX_VALUE + 1]).unwrap_err().code(), -2);
        for i in 0..15 {
            s.set(&format!("k{i:02}"), &vec![0; MAX_VALUE]).unwrap();
        }
        assert_eq!(s.set("one-more", &vec![0; MAX_VALUE]).unwrap_err().code(), -3);
        s.set("k00", b"small").unwrap(); // replacing frees the old value's bytes
        s.set("one-more", &vec![0; MAX_VALUE]).unwrap();
        assert!(s.delete("k01") && !s.delete("k01"));
        assert_eq!(s.keys().len(), 15);
    }
}
