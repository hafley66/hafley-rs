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

#[cfg(feature = "shared")]
pub use hafley_scm::read::*;
#[cfg(feature = "read")]
#[path = "edit/_2_drain.rs"]
pub mod drain;
#[cfg(feature = "read")]
pub mod edit;
#[cfg(feature = "read")]
#[path = "edit/_0_seams.rs"]
pub mod edit_seams;
#[cfg(feature = "read")]
#[path = "edit/_1_move_cx.rs"]
pub mod move_cx;
#[cfg(feature = "read")]
#[path = "edit/_4_move_scip.rs"]
pub mod move_scip;
#[cfg(feature = "read")]
#[path = "edit/_3_stage.rs"]
pub mod move_stage;
#[cfg(feature = "read")]
#[path = "edit/_1_rename_cx.rs"]
pub mod rename_cx;
/// The run trail rides the same subscriber the `cli` feature installs.
#[cfg(feature = "cli")]
pub mod trail;
#[cfg(feature = "read")]
pub use crate::edit::cleave_for;
#[cfg(feature = "read")]
pub use crate::edit::cleaves;
#[cfg(feature = "read")]
pub use crate::edit::rehome_for;
#[cfg(feature = "read")]
pub use crate::edit::rehomes;
#[cfg(feature = "read")]
pub use crate::edit::rename_for;
#[cfg(feature = "read")]
pub use crate::edit::renames;
#[cfg(feature = "read")]
pub use crate::edit_seams::Cleave;
#[cfg(feature = "read")]
pub use crate::edit_seams::Edit;
#[cfg(feature = "read")]
pub use crate::edit_seams::ImportRef;
#[cfg(feature = "read")]
pub use crate::edit_seams::ImportRefKind;
#[cfg(feature = "read")]
pub use crate::edit_seams::RefRole;
#[cfg(feature = "read")]
pub use crate::edit_seams::Rehome;
#[cfg(feature = "read")]
pub use crate::edit_seams::RehomeArm;
#[cfg(feature = "read")]
pub use crate::edit_seams::RehomeManifests;
#[cfg(feature = "read")]
pub use crate::edit_seams::RehomePlanCheck;
#[cfg(feature = "read")]
pub use crate::edit_seams::RehomeShim;
#[cfg(feature = "read")]
pub use crate::edit_seams::RehomeTextSpellings;
#[cfg(feature = "read")]
pub use crate::edit_seams::Rename;
#[cfg(feature = "read")]
pub use crate::edit_seams::RenameStop;
#[cfg(feature = "read")]
pub use crate::edit_seams::Respell;
#[cfg(feature = "read")]
pub use crate::edit_seams::SymbolRef;
#[cfg(feature = "read")]
pub use crate::edit_seams::SymbolSeat;
#[cfg(feature = "read")]
pub use drain::{
    bind_action, directory_path, directory_source, replace_action, source_rel, stage_edits,
};
#[cfg(feature = "read")]
pub use edit::ts_rehome::{build_paths, compiled_spellings, BuildPaths};
#[cfg(feature = "read")]
pub use move_cx::{dirname, join_rel, normalize, relative_between, MoveCx, SKIP_DIRS};
#[cfg(feature = "read")]
pub use move_scip::{
    scip_import_sites, verify_import_refs, ScipDisagreement, ScipSite, MISSED_BY_IMPL,
    UNKNOWN_TO_SCIP,
};
#[cfg(feature = "read")]
pub use rename_cx::{RenameCx, RenameRequest};
