//! Closed, content-neutral mechanics language.
//!
//! Static [`Program`] values carry authored identity and policy. The hot reducer reads a
//! rollback snapshot, plans a bounded [`CommandTape`], and commits later. Native Rust matches
//! operations and capabilities; it must not branch on character/item/material identities.
//!
//! See `COMPASS.md` beside this file and `plans/mechanics-language-tasks.md`.

mod document;
mod document_testing;
mod entity;
mod id;
mod program;
mod relation;
mod runtime;
mod set;
mod testing;
mod vector;
mod world;
mod world_testing;

pub use document::*;
pub use document_testing::*;
pub use entity::*;
pub use id::*;
pub use program::*;
pub use relation::*;
pub use runtime::*;
pub use set::*;
pub use testing::*;
pub use vector::*;
pub use world::*;
pub use world_testing::*;
