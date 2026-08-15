//! The monotone turn cursor: `<worktree>/state/cursor`, the max `ts` the pipe
//! has folded. Reads since it on the next cycle; writes advance it only after a
//! fold completes, so a crash never folds the same delta twice.

use std::path::Path;

use anyhow::{Context, Result};

/// The resume cursor for one source session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The max `ts` folded so far. New turns with `ts` strictly greater than
    /// this are new.
    pub max_ts: i64,
}

impl Cursor {
    /// Load the cursor from a file. A missing or empty file is cursor 0.
    pub fn load(path: &Path) -> Result<Cursor> {
        if !path.exists() {
            return Ok(Cursor::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read cursor {}", path.display()))?;
        let max_ts = text.trim().parse().unwrap_or(0);
        Ok(Cursor { max_ts })
    }

    /// Advance the cursor to cover `ts`, returning true if it moved.
    pub fn observe(&mut self, ts: i64) -> bool {
        if ts > self.max_ts {
            self.max_ts = ts;
            true
        } else {
            false
        }
    }

    /// Write the cursor file.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.max_ts.to_string())
            .with_context(|| format!("write cursor {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cursor::Cursor;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("concatmap_cursor_{}_{}", std::process::id(), name))
    }

    #[test]
    fn missing_file_is_zero() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Cursor::load(&path).unwrap(), Cursor::default());
    }

    #[test]
    fn observe_is_monotone() {
        let mut cursor = Cursor::default();
        assert!(cursor.observe(5));
        assert!(!cursor.observe(4), "older ts does not move the cursor");
        assert!(cursor.observe(7));
        assert_eq!(cursor.max_ts, 7);
    }

    #[test]
    fn save_load_round_trips() {
        let path = temp_path("roundtrip");
        let mut cursor = Cursor::default();
        cursor.observe(12);
        cursor.save(&path).unwrap();
        assert_eq!(Cursor::load(&path).unwrap().max_ts, 12);
        let _ = std::fs::remove_file(&path);
    }
}
