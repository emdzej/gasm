//! gasm:storage for the native runner: one directory per game, one file per key.
//!
//! Windowed runs persist under the OS data directory (`--storage-dir` overrides);
//! headless runs use an in-memory store unless a directory is given, so tests
//! stay reproducible. Writes go to a temp file and are renamed into place.

use std::collections::HashMap;
use std::path::PathBuf;

pub const MAX_KEY: usize = 128;
pub const MAX_VALUE: usize = 1 << 20;
pub const QUOTA: usize = 16 << 20;

pub struct Storage {
    dir: Option<PathBuf>,
    values: HashMap<String, Vec<u8>>,
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

impl Storage {
    pub fn memory() -> Storage {
        Storage { dir: None, values: HashMap::new() }
    }

    /// Open (creating if needed) a directory-backed store and load its values.
    pub fn open(dir: PathBuf) -> Result<Storage, String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut values = HashMap::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if valid_key(&name) && entry.path().is_file() {
                if let Ok(v) = std::fs::read(entry.path()) {
                    values.insert(name, v);
                }
            }
        }
        Ok(Storage { dir: Some(dir), values })
    }

    pub fn location(&self) -> String {
        self.dir.as_ref().map_or("memory".into(), |d| d.display().to_string())
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.values.get(key).map(Vec::as_slice)
    }

    pub fn set(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        if !valid_key(key) {
            return Err(format!("invalid key {key:?}"));
        }
        if value.len() > MAX_VALUE {
            return Err(format!("value for {key:?} is {} bytes (max {MAX_VALUE})", value.len()));
        }
        let used: usize = self.values.iter().filter(|(k, _)| k.as_str() != key).map(|(k, v)| k.len() + v.len()).sum();
        if used + key.len() + value.len() > QUOTA {
            return Err(format!("storage quota of {QUOTA} bytes exceeded"));
        }
        if let Some(dir) = &self.dir {
            let tmp = dir.join(format!(".{key}.tmp"));
            std::fs::write(&tmp, value).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, dir.join(key)).map_err(|e| e.to_string())?;
        }
        self.values.insert(key.to_owned(), value.to_vec());
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> bool {
        if self.values.remove(key).is_none() {
            return false;
        }
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(key));
        }
        true
    }
}
