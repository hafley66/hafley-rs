//! The uniform surface. Canonical definitions now live in `crate::read::types`; this
//! module is a re-export so `crate::read::source::*` import paths keep resolving.
//! Commit 4a: `Resolve` (the phase-2 extension of `Source`) + the `ProjectCx`
//! it resolves against ride here too, so a language binding implementing both
//! phases imports them from one place.
pub use crate::read::types::{RyiOutput, FamilyMask, ProjectCx, Resolve, Source};
