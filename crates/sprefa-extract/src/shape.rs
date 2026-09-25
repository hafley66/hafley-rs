//! S1 atoms. Canonical definitions now live in `crate::types`; this module is a
//! re-export so `crate::shape::*` import paths keep resolving.
pub use crate::types::{content_id_of, ContentId, FamilyTag, NameId, NodeRef, Strings, ZERO_CONTENT_ID};
pub use hafley_scm::span::Span;
