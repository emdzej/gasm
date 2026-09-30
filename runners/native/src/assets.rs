//! Read-only guest assets: in-memory buffers, or files opened (not read) at start.
//!
//! - **File-backed** (`--asset`, `--rom`, `--asset-dir`): `size` comes from file
//!   metadata and reads are positioned (`pread` on Unix, `seek_read` on
//!   Windows) straight into guest memory. Nothing is loaded up front, so a
//!   200 MB asset costs no RAM and no start-up time. A file that shrinks or
//!   disappears while running just yields fewer bytes (possibly 0); reads never
//!   panic.
//! - **Folders** (`--asset-dir [prefix=]dir`): every regular file under `dir`,
//!   named by its `/`-separated relative path as stored on disk. Symlinks are
//!   skipped (so nothing outside `dir` is reachable) and so are hidden entries
//!   (any path component starting with `.`, e.g. `.DS_Store`, `._foo`). The set
//!   of names is fixed at start-up.
//! - **Lookup:** an exact name always wins (explicit `--asset` entries override
//!   folder entries of the same name). Otherwise folder entries also match
//!   **case-insensitively** (ASCII folding): CD file systems are upper case,
//!   games ask in mixed case. If several folder entries differ only in case, a
//!   warning is logged at start-up and the first in sorted order wins.
//! - Sizes are 32-bit in the ABI: files over 2 GiB - 1 are refused.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Largest asset the ABI can address (`asset_size -> i32`).
pub const MAX_ASSET: u64 = i32::MAX as u64;

enum Source {
    Memory(Vec<u8>),
    File { file: File, path: PathBuf },
}

struct Entry {
    source: Source,
    from_dir: bool,
}

#[derive(Default)]
pub struct Assets {
    exact: HashMap<String, Entry>,
    /// ASCII-lowercased name -> folder entry names, sorted
    folded: BTreeMap<String, Vec<String>>,
}

impl From<HashMap<String, Vec<u8>>> for Assets {
    fn from(map: HashMap<String, Vec<u8>>) -> Assets {
        let mut a = Assets::default();
        for (name, bytes) in map {
            a.insert_memory(&name, bytes);
        }
        a
    }
}

fn open_checked(path: &Path) -> Result<(File, u64), String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len > MAX_ASSET {
        return Err(format!(
            "{}: {len} bytes is larger than the 2 GiB the gasm ABI can address (asset sizes are 32-bit)",
            path.display()
        ));
    }
    Ok((file, len))
}

#[cfg(unix)]
fn pread(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn pread(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

impl Assets {
    pub fn new() -> Assets {
        Assets::default()
    }

    pub fn len(&self) -> usize {
        self.exact.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }

    /// Asset from memory (overrides any entry with the same name).
    pub fn insert_memory(&mut self, name: &str, bytes: Vec<u8>) {
        self.exact.insert(name.to_owned(), Entry { source: Source::Memory(bytes), from_dir: false });
    }

    /// Asset backed by a file, opened now and read on demand (overrides any
    /// entry with the same name).
    pub fn insert_file(&mut self, name: &str, path: &Path) -> Result<(), String> {
        let (file, _) = open_checked(path)?;
        self.exact.insert(name.to_owned(), Entry { source: Source::File { file, path: path.to_owned() }, from_dir: false });
        Ok(())
    }

    /// Expose every regular file under `dir` (recursively), named
    /// `[prefix/]relative/path`. Existing names are kept (explicit assets win).
    /// Returns the number of files added. Call [`Assets::finish`] afterwards.
    pub fn add_dir(&mut self, prefix: Option<&str>, dir: &Path) -> Result<usize, String> {
        let meta = std::fs::metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        if !meta.is_dir() {
            return Err(format!("{}: not a directory", dir.display()));
        }
        let mut files = Vec::new();
        walk(dir, &mut Vec::new(), &mut files)?;
        let mut added = 0;
        for (segments, path) in files {
            let rel = segments.join("/");
            let name = match prefix {
                Some(p) if !p.is_empty() => format!("{}/{rel}", p.trim_end_matches('/')),
                _ => rel,
            };
            if self.exact.contains_key(&name) {
                continue;
            }
            match open_checked(&path) {
                Ok((file, _)) => {
                    self.exact.insert(name, Entry { source: Source::File { file, path }, from_dir: true });
                    added += 1;
                }
                Err(e) => eprintln!("[gasm] assets: skipping {e}"),
            }
        }
        Ok(added)
    }

    /// Build the case-insensitive index for folder entries; logs collisions.
    pub fn finish(&mut self) {
        self.folded.clear();
        for (name, entry) in &self.exact {
            if entry.from_dir {
                self.folded.entry(name.to_ascii_lowercase()).or_default().push(name.clone());
            }
        }
        for names in self.folded.values_mut() {
            names.sort();
            if names.len() > 1 {
                eprintln!(
                    "[gasm] assets: {} differ only in case; case-insensitive lookups use {:?}",
                    names.join(", "),
                    names[0]
                );
            }
        }
    }

    fn resolve(&self, name: &str) -> Option<&Entry> {
        if let Some(e) = self.exact.get(name) {
            return Some(e);
        }
        let canonical = self.folded.get(&name.to_ascii_lowercase())?.first()?;
        self.exact.get(canonical)
    }

    /// Current size in bytes, or `None` if there is no such asset (or its file vanished).
    pub fn size(&self, name: &str) -> Option<u64> {
        match &self.resolve(name)?.source {
            Source::Memory(b) => Some(b.len() as u64),
            Source::File { file, .. } => file.metadata().ok().map(|m| m.len().min(MAX_ASSET)),
        }
    }

    /// Copy bytes starting at `offset` into `dst`. Returns bytes copied (0 at or
    /// past the end), or `None` if there is no such asset. Short reads (a file
    /// that shrank) return what was available; I/O errors end the read early.
    pub fn read_at(&self, name: &str, offset: u64, dst: &mut [u8]) -> Option<usize> {
        match &self.resolve(name)?.source {
            Source::Memory(b) => {
                let start = (offset as usize).min(b.len());
                let n = (b.len() - start).min(dst.len());
                dst[..n].copy_from_slice(&b[start..start + n]);
                Some(n)
            }
            Source::File { file, path } => {
                let mut done = 0;
                while done < dst.len() {
                    match pread(file, &mut dst[done..], offset + done as u64) {
                        Ok(0) => break,
                        Ok(n) => done += n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(e) => {
                            eprintln!("[gasm] assets: {}: {e}", path.display());
                            break;
                        }
                    }
                }
                Some(done)
            }
        }
    }

    /// All asset names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.exact.keys().cloned().collect();
        v.sort();
        v
    }
}

/// Recursive, sorted walk collecting regular files; skips symlinks and hidden
/// entries. Names must be valid UTF-8 (others are skipped with a warning).
fn walk(dir: &Path, prefix: &mut Vec<String>, out: &mut Vec<(Vec<String>, PathBuf)>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            eprintln!("[gasm] assets: skipping non-UTF-8 name {:?}", entry.path());
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue }; // does not follow symlinks
        if ft.is_symlink() {
            continue;
        }
        prefix.push(name);
        if ft.is_dir() {
            walk(&entry.path(), prefix, out)?;
        } else if ft.is_file() {
            out.push((prefix.clone(), entry.path()));
        }
        prefix.pop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let d = std::env::temp_dir().join(format!("gasm-assets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("ART")).unwrap();
        std::fs::create_dir_all(d.join("WORLDS/1PLAYER")).unwrap();
        std::fs::write(d.join("ART/ART.CAR"), b"car").unwrap();
        std::fs::write(d.join("WORLDS/1PLAYER/MAP.RFM"), b"map-data").unwrap();
        std::fs::write(d.join(".DS_Store"), b"x").unwrap();
        std::fs::write(d.join("dup.txt"), b"lower").unwrap();
        std::fs::write(d.join("DUP.TXT"), b"upper").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", d.join("escape")).unwrap();
        d
    }

    #[test]
    fn folders_case_and_precedence() {
        let d = tree();
        let mut a = Assets::new();
        a.insert_memory("ART/ART.CAR", b"explicit".to_vec());
        a.add_dir(None, &d).unwrap();
        a.add_dir(Some("cd"), &d).unwrap();
        a.finish();
        let read = |a: &Assets, n: &str| {
            let mut buf = vec![0u8; 64];
            a.read_at(n, 0, &mut buf).map(|k| String::from_utf8_lossy(&buf[..k]).into_owned())
        };
        assert_eq!(read(&a, "ART/ART.CAR").as_deref(), Some("explicit")); // explicit wins
        assert_eq!(read(&a, "cd/art/art.car").as_deref(), Some("car")); // case-insensitive
        assert_eq!(read(&a, "Worlds/1player/map.rfm").as_deref(), Some("map-data"));
        // Case collisions only exist on case-sensitive file systems (not default macOS/Windows).
        let case_sensitive = std::fs::read_dir(&d).unwrap().filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("dup.txt")).count() == 2;
        if case_sensitive {
            assert_eq!(read(&a, "dup.txt").as_deref(), Some("lower")); // exact beats folding
            assert_eq!(read(&a, "Dup.Txt").as_deref(), Some("upper")); // first in sorted order ("DUP.TXT" < "dup.txt")
        }
        assert!(a.size(".DS_Store").is_none()); // hidden skipped
        assert!(a.size("escape").is_none()); // symlink skipped
        assert_eq!(a.size("worlds/1player/MAP.RFM"), Some(8));
        let mut buf = [0u8; 4];
        assert_eq!(a.read_at("WORLDS/1PLAYER/MAP.RFM", 4, &mut buf), Some(4));
        assert_eq!(&buf, b"data");
        assert_eq!(a.read_at("WORLDS/1PLAYER/MAP.RFM", 100, &mut buf), Some(0)); // past the end
        assert!(a.read_at("missing", 0, &mut buf).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn shrinking_file_never_panics() {
        let p = std::env::temp_dir().join(format!("gasm-shrink-{}", std::process::id()));
        std::fs::write(&p, vec![7u8; 1000]).unwrap();
        let mut a = Assets::new();
        a.insert_file("f", &p).unwrap();
        std::fs::write(&p, vec![7u8; 10]).unwrap(); // truncate while "running"
        let mut buf = vec![0u8; 100];
        assert_eq!(a.size("f"), Some(10));
        assert_eq!(a.read_at("f", 0, &mut buf), Some(10));
        assert_eq!(a.read_at("f", 500, &mut buf), Some(0));
        std::fs::remove_file(&p).unwrap(); // vanishes
        let _ = a.read_at("f", 0, &mut buf); // must not panic (Unix keeps the inode)
        let _ = std::fs::remove_file(&p);
    }
}
