//! S1 atoms. Canonical definitions now live in `crate::read::types`; this module is a
//! re-export so `crate::read::shape::*` import paths keep resolving.
pub use crate::read::types::{content_id_of, ContentId, ZERO_CONTENT_ID};
pub use hafley_scm::span::Span;
pub use hafley_scm::atoms::NameId;
pub use hafley_scm::atoms::Strings;
pub use hafley_scm::atoms::NodeRef;
pub use hafley_scm::atoms::FamilyTag;
