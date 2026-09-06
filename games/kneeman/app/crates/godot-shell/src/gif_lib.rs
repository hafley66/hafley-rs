// scaffolding: pending wire-or-delete ruling — task G2 built, no caller yet (see report table)
#![allow(dead_code)]

//! Gif background library: plain `std::fs` storage over an injected root dir. Prod passes the
//! `user://gifs/` path (mapped to a real dir on native, IndexedDB-backed on web, via whatever
//! wraps Godot's `user://` elsewhere in the shell); tests pass a tempdir. No Godot objects here —
//! see plans/gif-background-library.md, task G2, item 1.
//!
//! Decode is NOT this module's job: `list_gifs` only scans names, so a picker can show a grid
//! without decoding every file up front. Callers decode-on-demand (via `gif.rs::decode_gif`) for
//! whichever entry needs a thumbnail.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One entry in the library: display name (the file stem) + where the bytes live on disk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GifEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Where `name`'s bytes would live under `root`. Names are stored as `<name>.gif`; callers pass
/// the bare name (no extension) the way `list_gifs` hands them back.
fn gif_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}.gif"))
}

/// Write `bytes` to `root/<name>.gif`, creating `root` if it doesn't exist yet (a fresh library
/// has no directory until the first save). Overwrites an existing file of the same name.
pub fn save_gif(root: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    fs::create_dir_all(root)?;
    let path = gif_path(root, name);
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Read back `root/<name>.gif` for decode. `Err` (not a panic) if the name was never saved.
pub fn load_gif(root: &Path, name: &str) -> io::Result<Vec<u8>> {
    fs::read(gif_path(root, name))
}

/// Is `path` a gif by extension? Case-insensitive `.gif` — enough to skip stray non-gif files
/// dropped into the library dir without decoding anything.
fn is_gif_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"))
}

/// List every `.gif` file directly under `root`, name + path. A missing `root` (nothing saved
/// yet) reads as an empty library, not an error. Non-gif files and subdirectories are skipped,
/// never panicked on; thumbnails are the caller's job (decode-on-demand), not decoded here.
pub fn list_gifs(root: &Path) -> Vec<GifEntry> {
    let entries = match fs::read_dir(root) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_gif_path(&path) {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue; // non-UTF8 stem: skip rather than lossy-guess a display name
        };
        out.push(GifEntry {
            name: name.to_string(),
            path,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{list_gifs, load_gif, save_gif};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A tempdir under `std::env::temp_dir()` with a unique suffix, removed on drop so tests
    /// never leak files into the system temp dir.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "smash-gif-lib-test-{tag}-{}-{n}",
                std::process::id()
            ));
            TempDir(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // 1. save_gif/load_gif/list_gifs round-trip through an injected root dir.
    #[test]
    fn save_load_list_roundtrip() {
        let dir = TempDir::new("roundtrip");
        let root = dir.path();
        let bytes = b"GIF89a...fake pixels...".as_slice();

        save_gif(root, "sunset", bytes).expect("save creates the dir and writes the file");
        assert_eq!(load_gif(root, "sunset").expect("load reads it back"), bytes);

        let listed = list_gifs(root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "sunset");
        assert_eq!(listed[0].path, root.join("sunset.gif"));
    }

    // 2. list_gifs on a dir containing a non-gif file skips it: no panic, not listed.
    #[test]
    fn list_gifs_skips_non_gif_files() {
        let dir = TempDir::new("skip-non-gif");
        let root = dir.path();

        save_gif(root, "keeper", b"gif bytes").unwrap();
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("notes.txt"), b"not a gif").unwrap();

        let listed = list_gifs(root);
        assert_eq!(listed.len(), 1); // only the .gif file is listed
        assert_eq!(listed[0].name, "keeper");
    }

    // list_gifs on a root that was never created (nothing saved yet) is an empty library, not
    // a panic or an Err the caller has to handle.
    #[test]
    fn list_gifs_on_missing_root_is_empty() {
        let dir = TempDir::new("missing-root");
        assert!(list_gifs(dir.path()).is_empty());
    }

    // load_gif of a name that was never saved is an Err, not a panic.
    #[test]
    fn load_gif_of_unknown_name_errs() {
        let dir = TempDir::new("unknown-name");
        std::fs::create_dir_all(dir.path()).unwrap();
        assert!(load_gif(dir.path(), "nope").is_err());
    }
}
