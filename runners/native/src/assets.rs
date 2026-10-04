//! Read-only guest assets: in-memory buffers, or files read on demand.
//!
//! - **File-backed** (`--asset`, `--rom`, `--asset-dir`): the size comes from file
//!   metadata and reads are positioned (`pread` on Unix, `seek_read` on
//!   Windows) straight into guest memory. Nothing is loaded up front, so a
//!   200 MB asset costs no RAM and no start-up time. A file that shrinks or
//!   disappears while running just yields fewer bytes (possibly 0); reads never
//!   panic. Explicit files are opened at start; folder entries are opened on
//!   first read and kept in a small cache, so a CD-sized tree doesn't hit the
//!   open-file limit.
//! - **Folders** (`--asset-dir [prefix=]dir`): every regular file under `dir`,
//!   named by its `/`-separated relative path as stored on disk. Symlinks are
//!   skipped (so nothing outside `dir` is reachable; a file replaced by a
//!   symlink later is refused too) and so are hidden entries (any path
//!   component starting with `.`, e.g. `.DS_Store`, `._foo`). The set of names
//!   is fixed at start-up.
//! - **Lookup:** an exact name always wins (explicit `--asset` entries override
//!   folder entries of the same name). Otherwise folder entries also match
//!   **case-insensitively** (ASCII folding): CD file systems are upper case,
//!   games ask in mixed case. If several folder entries differ only in case, a
//!   warning is logged at start-up and the first in sorted order wins.
//! - Sizes are 64-bit (`asset_size64`, `asset_read_at64`); `asset_size`
//!   reports assets of 2 GiB and more as -2.
//! - **Replacing assets while the game runs:** the embedder calls
//!   [`Assets::set`] / [`Assets::set_file`] / [`Assets::remove`] between frames
//!   (`Game::set_asset`, or `--watch-asset`, which re-opens a file whenever it
//!   changes: [`AssetWatch`]). Each replacement gives the asset a new version
//!   (`gasm.asset_version`): 0 for assets given at start, then increasing
//!   numbers from one counter, so a removed and re-added asset never repeats one.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Folder entries kept open at once (least recently used are closed).
const OPEN_FILES: usize = 64;

enum Source {
    Memory(Vec<u8>),
    /// explicit file: opened at start (the path the user named)
    File { file: File, path: PathBuf },
    /// folder entry: opened on first read
    Lazy { path: PathBuf, len: u64 },
}

struct Entry {
    source: Source,
    from_dir: bool,
    version: u32,
}

#[derive(Default)]
pub struct Assets {
    exact: HashMap<String, Entry>,
    /// ASCII-lowercased name -> folder entry names, sorted
    folded: BTreeMap<String, Vec<String>>,
    /// sorted names, built by `finish`
    sorted: Vec<String>,
    /// open folder entries, most recently used last
    open: RefCell<VecDeque<(PathBuf, Arc<File>)>>,
    /// the last version handed out by a replacement (assets given at start are 0)
    last_version: u32,
}

impl From<HashMap<String, Vec<u8>>> for Assets {
    fn from(map: HashMap<String, Vec<u8>>) -> Assets {
        let mut a = Assets::default();
        for (name, bytes) in map {
            a.insert_memory(&name, bytes);
        }
        a.finish();
        a
    }
}

#[cfg(unix)]
fn pread(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn pread(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

/// Fill `dst` from `offset`; stops early at the end of the file or on an error.
fn read_file(file: &File, path: &Path, offset: u64, dst: &mut [u8]) -> usize {
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
    done
}

/// A resolved asset: its size and reads, without looking the name up again.
pub struct Asset<'a> {
    assets: &'a Assets,
    source: &'a Source,
    version: u32,
}

impl Asset<'_> {
    /// 0 if given at start, else a new number each time it was replaced.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Size in bytes (for folder entries: as found at start-up).
    pub fn size(&self) -> u64 {
        match self.source {
            Source::Memory(b) => b.len() as u64,
            Source::File { file, .. } => file.metadata().map_or(0, |m| m.len()),
            Source::Lazy { len, .. } => *len,
        }
    }

    /// Copy bytes starting at `offset` into `dst`. Returns bytes copied (0 at or
    /// past the end). Short reads (a file that shrank) return what was
    /// available; I/O errors end the read early.
    pub fn read_at(&self, offset: u64, dst: &mut [u8]) -> usize {
        match self.source {
            Source::Memory(b) => {
                let start = offset.min(b.len() as u64) as usize;
                let n = (b.len() - start).min(dst.len());
                dst[..n].copy_from_slice(&b[start..start + n]);
                n
            }
            Source::File { file, path } => read_file(file, path, offset, dst),
            Source::Lazy { path, .. } => match self.assets.open_lazy(path) {
                Some(file) => read_file(&file, path, offset, dst),
                None => 0,
            },
        }
    }
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
        self.exact.insert(name.to_owned(), Entry { source: Source::Memory(bytes), from_dir: false, version: 0 });
    }

    /// Asset backed by a file, opened now and read on demand (overrides any
    /// entry with the same name).
    pub fn insert_file(&mut self, name: &str, path: &Path) -> Result<(), String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.is_file() {
            return Err(format!("{}: not a regular file", path.display()));
        }
        self.exact.insert(name.to_owned(), Entry { source: Source::File { file, path: path.to_owned() }, from_dir: false, version: 0 });
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
        for (segments, path, len) in files {
            let rel = segments.join("/");
            let name = match prefix {
                Some(p) if !p.is_empty() => format!("{}/{rel}", p.trim_end_matches('/')),
                _ => rel,
            };
            if self.exact.contains_key(&name) {
                continue;
            }
            self.exact.insert(name, Entry { source: Source::Lazy { path, len }, from_dir: true, version: 0 });
            added += 1;
        }
        Ok(added)
    }

    /// Add or replace an asset from memory while the game runs (between frames):
    /// it gets a new version. Returns that version.
    pub fn set(&mut self, name: &str, bytes: Vec<u8>) -> u32 {
        self.replace(name, Source::Memory(bytes))
    }

    /// [`Assets::set`] with a file read on demand (opened now).
    pub fn set_file(&mut self, name: &str, path: &Path) -> Result<u32, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.is_file() {
            return Err(format!("{}: not a regular file", path.display()));
        }
        Ok(self.replace(name, Source::File { file, path: path.to_owned() }))
    }

    /// Remove an asset while the game runs (between frames). False if there was none.
    pub fn remove(&mut self, name: &str) -> bool {
        let gone = self.exact.remove(name).is_some();
        if gone {
            self.index();
        }
        gone
    }

    fn replace(&mut self, name: &str, source: Source) -> u32 {
        self.last_version += 1;
        self.exact.insert(name.to_owned(), Entry { source, from_dir: false, version: self.last_version });
        self.index();
        self.last_version
    }

    /// Build the case-insensitive index and the sorted name list; logs collisions.
    pub fn finish(&mut self) {
        self.index();
        for names in self.folded.values() {
            if names.len() > 1 {
                eprintln!(
                    "[gasm] assets: {} differ only in case; case-insensitive lookups use {:?}",
                    names.join(", "),
                    names[0]
                );
            }
        }
    }

    fn index(&mut self) {
        self.folded.clear();
        for (name, entry) in &self.exact {
            if entry.from_dir {
                self.folded.entry(name.to_ascii_lowercase()).or_default().push(name.clone());
            }
        }
        for names in self.folded.values_mut() {
            names.sort();
        }
        self.sorted = self.exact.keys().cloned().collect();
        self.sorted.sort();
    }

    /// Look an asset up (exact name, then case-insensitively among folder entries).
    pub fn get(&self, name: &str) -> Option<Asset<'_>> {
        let entry = match self.exact.get(name) {
            Some(e) => e,
            None => self.exact.get(self.folded.get(&name.to_ascii_lowercase())?.first()?)?,
        };
        Some(Asset { assets: self, source: &entry.source, version: entry.version })
    }

    /// Current size in bytes, or `None` if there is no such asset.
    pub fn size(&self, name: &str) -> Option<u64> {
        self.get(name).map(|a| a.size())
    }

    /// Copy bytes starting at `offset` into `dst` (see [`Asset::read_at`]), or
    /// `None` if there is no such asset.
    pub fn read_at(&self, name: &str, offset: u64, dst: &mut [u8]) -> Option<usize> {
        self.get(name).map(|a| a.read_at(offset, dst))
    }

    /// The asset's version (see [`Asset::version`]), or `None` if there is no such asset.
    pub fn version(&self, name: &str) -> Option<u32> {
        self.get(name).map(|a| a.version())
    }

    /// All asset names, sorted (by UTF-8 bytes).
    pub fn names(&self) -> &[String] {
        &self.sorted
    }

    /// Open a folder entry (cached). Refuses anything that is no longer a regular
    /// file (e.g. replaced by a symlink since start-up).
    fn open_lazy(&self, path: &Path) -> Option<Arc<File>> {
        let mut open = self.open.borrow_mut();
        if let Some(i) = open.iter().position(|(p, _)| p == path) {
            let hit = open.remove(i)?;
            let file = hit.1.clone();
            open.push_back(hit);
            return Some(file);
        }
        let ok = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file());
        let file = match ok.then(|| File::open(path)) {
            Some(Ok(f)) => Arc::new(f),
            Some(Err(e)) => {
                eprintln!("[gasm] assets: {}: {e}", path.display());
                return None;
            }
            None => {
                eprintln!("[gasm] assets: {}: no longer a regular file", path.display());
                return None;
            }
        };
        if open.len() >= OPEN_FILES {
            open.pop_front();
        }
        open.push_back((path.to_owned(), file.clone()));
        Some(file)
    }
}

/// `--watch-asset name=path`: the file is re-opened as asset `name` whenever its
/// size or modification time changes (in place or replaced by a rename), checked
/// before each frame. A file that disappears keeps the last version until it's back.
pub struct AssetWatch {
    pub name: String,
    pub path: PathBuf,
    stamp: Option<(std::time::SystemTime, u64)>,
}

impl AssetWatch {
    /// Watch `path`; the asset itself is added by [`AssetWatch::poll`] (or already given at start).
    pub fn new(name: &str, path: &Path) -> AssetWatch {
        AssetWatch { name: name.to_owned(), path: path.to_owned(), stamp: Self::stamp(path) }
    }

    fn stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
        let m = std::fs::metadata(path).ok()?;
        Some((m.modified().ok()?, m.len()))
    }

    /// Re-open the file if it changed. Returns the new version, if any.
    pub fn poll(&mut self, assets: &mut Assets) -> Option<u32> {
        let now = Self::stamp(&self.path)?;
        if self.stamp == Some(now) && assets.get(&self.name).is_some() {
            return None;
        }
        self.stamp = Some(now);
        match assets.set_file(&self.name, &self.path) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("[gasm] assets: --watch-asset {e}");
                None
            }
        }
    }
}

/// Recursive, sorted walk collecting regular files (with their sizes); skips
/// symlinks and hidden entries. Names must be valid UTF-8 (others are skipped
/// with a warning).
fn walk(dir: &Path, prefix: &mut Vec<String>, out: &mut Vec<(Vec<String>, PathBuf, u64)>) -> Result<(), String> {
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
            match entry.metadata() {
                Ok(m) => out.push((prefix.clone(), entry.path(), m.len())),
                Err(e) => eprintln!("[gasm] assets: skipping {}: {e}", entry.path().display()),
            }
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
    fn replace_and_watch() {
        let read = |a: &Assets, n: &str| {
            let mut buf = vec![0u8; 64];
            a.read_at(n, 0, &mut buf).map(|k| String::from_utf8_lossy(&buf[..k]).into_owned())
        };
        let mut a = Assets::new();
        a.insert_memory("weather.json", b"{}".to_vec());
        a.finish();
        assert_eq!(a.version("weather.json"), Some(0));
        assert_eq!(a.set("weather.json", b"{\"t\":1}".to_vec()), 1);
        assert_eq!((a.version("weather.json"), read(&a, "weather.json").as_deref()), (Some(1), Some("{\"t\":1}")));
        assert_eq!(a.set("tiles/0.png", b"png".to_vec()), 2); // a new name is listed
        assert_eq!(a.names(), ["tiles/0.png", "weather.json"]);
        assert!(a.remove("tiles/0.png") && !a.remove("tiles/0.png"));
        assert_eq!((a.version("tiles/0.png"), a.names().len()), (None, 1));
        assert_eq!(a.set("tiles/0.png", b"png".to_vec()), 3); // never a version seen before

        let d = std::env::temp_dir().join(format!("gasm-assets-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("w.json");
        std::fs::write(&f, b"one").unwrap();
        let mut w = AssetWatch::new("w.json", &f);
        assert_eq!(w.poll(&mut a), Some(4)); // not an asset yet: added
        assert_eq!(w.poll(&mut a), None); // unchanged
        std::fs::write(d.join("tmp"), b"second").unwrap();
        std::fs::rename(d.join("tmp"), &f).unwrap(); // atomic replace: a new size
        assert_eq!(w.poll(&mut a), Some(5));
        assert_eq!(read(&a, "w.json").as_deref(), Some("second"));
        std::fs::remove_file(&f).unwrap();
        assert_eq!(w.poll(&mut a), None); // gone: the last version stays
        assert_eq!(read(&a, "w.json").as_deref(), Some("second"));
        let _ = std::fs::remove_dir_all(&d);
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
        assert!(a.names().windows(2).all(|w| w[0] < w[1]));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn many_folder_files_stay_under_the_open_file_limit() {
        let d = std::env::temp_dir().join(format!("gasm-assets-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..(OPEN_FILES * 3) {
            std::fs::write(d.join(format!("F{i:04}")), format!("{i}")).unwrap();
        }
        let mut a = Assets::new();
        a.add_dir(None, &d).unwrap();
        a.finish();
        let mut buf = [0u8; 8];
        for round in 0..2 {
            for i in 0..(OPEN_FILES * 3) {
                let n = a.read_at(&format!("f{i:04}"), 0, &mut buf).unwrap();
                assert_eq!(&buf[..n], format!("{i}").as_bytes(), "round {round}");
            }
        }
        assert!(a.open.borrow().len() <= OPEN_FILES);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn shrinking_file_never_panics() {
        let p = std::env::temp_dir().join(format!("gasm-shrink-{}", std::process::id()));
        std::fs::write(&p, vec![7u8; 1000]).unwrap();
        let mut a = Assets::new();
        a.insert_file("f", &p).unwrap();
        a.finish();
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
