//! The ONE corpus read an `extract rename` run makes: the walk, the file set,
//! and the batch every `Rename` impl answers against. No language is named here.
//! @comment-ok: module header, the seam list every rename file opens with
//!
//! `MoveCx`'s twin, not an extension of it: a move batch is `path -> path` and a
//! rename batch is `(anchor, old) -> new`, so one context per verb keeps every
//! method free of a field it ignores. Same walker, same `SKIP_DIRS`, same
//! root-relative spelling law (`move_cx.rs:26,45,158`).

use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use std::cell::RefCell;

use crate::move_cx::walk_files;
use crate::edit_seams::Rename;

/// Whether the roster hands `rel` to `rename`.
pub fn owned_by<R: Rename + ?Sized>(rel: &str, rename: &R) -> bool {
    crate::edit::rename_for(rel).is_some_and(|owner| owner.name() == rename.name())
}

/// One symbol this run renames. The anchor names the DECLARING file; the
/// declaration in it is found by name, or by `at` when the name is declared twice.
#[derive(Clone)]
pub struct RenameRequest {
    /// Project-relative path of the declaring file.
    pub anchor: String,
    /// The identifier as written today.
    pub old: String,
    /// What it becomes.
    pub new: String,
    /// Byte offset INSIDE the declaration, when `old` is ambiguous.
    pub at: Option<u32>,
}

/// One `extract rename` run's corpus view, built once and borrowed by every
/// `Rename` impl.
pub struct RenameCx {
    root: PathBuf,
    files: Vec<String>,
    batch: Vec<RenameRequest>,
    overlay: BTreeMap<String, String>,
    rust_parse: RefCell<BTreeMap<String, syn::File>>,
}

impl RenameCx {
    /// One walk of `root`. `root` is taken canonicalized; every path this type
    /// hands out is root-relative and forward-slashed.
    pub fn open(root: &Path) -> Result<Self, String> {
        let files = walk_files(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            files,
            batch: Vec::new(),
            overlay: BTreeMap::new(),
            rust_parse: RefCell::new(BTreeMap::new()),
        })
    }

    /// The batch this run applies. Set once, before any impl is asked anything.
    pub fn with_batch(mut self, batch: Vec<RenameRequest>) -> Self {
        self.batch = batch;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Every corpus file `arm` owns, in path order. Ownership is the roster's
    /// first-match law, never an extension test here.
    pub fn files_of(&self, arm: &dyn Rename) -> Vec<&str> {
        self.files
            .iter()
            .map(String::as_str)
            .filter(|rel| owned_by(rel, arm))
            .collect()
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        self.overlay.get(rel).map(|text| text.as_bytes().to_vec())
            .or_else(|| std::fs::read(self.abs(rel)).ok())
    }

    pub fn text(&self, rel: &str) -> Option<String> {
        String::from_utf8(self.read(rel)?).ok()
    }

    pub fn batch(&self) -> &[RenameRequest] {
        &self.batch
    }

    pub fn overlay(&mut self, rel: String, text: String) {
        self.rust_parse.get_mut().remove(&rel);
        self.overlay.insert(rel, text);
    }

    /// Parse one version of a Rust file once across every list row that reads
    /// it. An overlay invalidates that file's parsed version.
    pub fn with_rust_parse<T>(&self, rel: &str, read: impl FnOnce(&syn::File) -> T) -> Option<T> {
        if !self.rust_parse.borrow().contains_key(rel) {
            let parsed = syn::parse_file(&self.text(rel)?).ok()?;
            self.rust_parse.borrow_mut().insert(rel.to_string(), parsed);
        }
        let parsed = self.rust_parse.borrow();
        Some(read(parsed.get(rel)?))
    }

    pub fn overlaid(&self) -> &BTreeMap<String, String> {
        &self.overlay
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}
