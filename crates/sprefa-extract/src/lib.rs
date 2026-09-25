//! sprefa-extract: a corpus at a version -> normalized graph facts. ONE sync leaf.
//!
//! Pure, SYNC, CPU-bound, arena-mastered. No database, no async, no reactor; the
//! async-eval flip + reactivity live in other crates (this iteration the
//! reactivity is an RxJS prototype that drives the CLI bin). The store sits
//! ABOVE this crate; extract never names a store id or a storage type (the
//! crate-map boundary rail).
//!
//! The lock and the build sequence live in
//! `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`; the canonical current
//! mind is `v6/sprefa-seed/src/_3_extract/_7_tasks.rs`.
//!
//! Commit 1 (this crate's first commit) is the PIPING PROOF: one Parser
//! (`RyiLang::tree_sitter_language`, linked grammars) + `Project<CstF>` (the
//! lossless named-node tree) + the flat wire + a clap bin streaming JSONL +
//! `--bench`. Proves bin -> seams -> flat wire -> stdout end to end.
#![allow(dead_code)]

pub use hafley_scm::read::*;
pub mod drain;
pub mod move_cx;
pub mod move_scip;
pub mod move_stage;
pub mod rename_cx;
/// The run trail rides the same subscriber the `cli` feature installs.
#[cfg(feature = "cli")]
pub mod trail;
#[path = "0_edit_seams.rs"] pub mod edit_seams;
pub mod edit;
pub use drain::{
    bind_action, directory_path, directory_source, replace_action, source_rel,
    stage_edits,
};
pub use move_cx::{dirname, join_rel, normalize, relative_between, MoveCx, SKIP_DIRS};
pub use move_scip::{
    scip_import_sites, verify_import_refs, ScipDisagreement, ScipSite, MISSED_BY_IMPL,
    UNKNOWN_TO_SCIP,
};
pub use rename_cx::{RenameCx, RenameRequest};
pub use crate::edit_seams::ImportRef;
pub use crate::edit_seams::ImportRefKind;
pub use crate::edit_seams::Respell;
pub use crate::edit_seams::Edit;
pub use crate::edit_seams::Cleave;
pub use crate::edit_seams::Rehome;
pub use crate::edit_seams::RehomeManifests;
pub use crate::edit_seams::RehomeShim;
pub use crate::edit_seams::RehomeTextSpellings;
pub use crate::edit_seams::RehomePlanCheck;
pub use crate::edit_seams::RehomeArm;
pub use crate::edit_seams::SymbolRef;
pub use crate::edit_seams::RefRole;
pub use crate::edit_seams::SymbolSeat;
pub use crate::edit_seams::RenameStop;
pub use crate::edit_seams::Rename;
pub use crate::edit::rehomes;
pub use crate::edit::renames;
pub use crate::edit::rehome_for;
pub use crate::edit::rename_for;
pub use crate::edit::cleaves;
pub use crate::edit::cleave_for;
pub use edit::ts_rehome::{build_paths, compiled_spellings, BuildPaths};
