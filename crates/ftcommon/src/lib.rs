//! Character-independent port of shared Melee `ftCommon` callbacks.
//!
//! Authored boundary types live in [`0_types.rs`](crate); the generated
//! translation of pinned decomp functions lives in `generated/0_ftcommon.rs`
//! and retains repository/revision/path/line-span provenance.

#[path = "0_types.rs"]
mod types;
pub use types::*;

#[path = "generated/0_ftcommon.rs"]
mod generated;
pub use generated::*;
