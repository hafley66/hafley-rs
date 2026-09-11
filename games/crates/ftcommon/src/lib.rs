//! Character-independent fighter transition mechanics generated from pinned
//! source evidence.
//!
//! The authored neutral contract lives in [`0_types.rs`](crate); generated
//! neutral rule data (with source provenance) lives in
//! `generated/0_ftcommon.rs`; the independently structured evaluator lives in
//! [`1_evaluate.rs`](crate) and interprets that data. Source identities stay in
//! the generated provenance, not in runtime declarations.

#[path = "0_types.rs"]
mod types;
pub use types::*;

#[path = "generated/0_ftcommon.rs"]
mod generated;
pub use generated::*;

#[path = "1_evaluate.rs"]
mod evaluate;
pub use evaluate::*;
