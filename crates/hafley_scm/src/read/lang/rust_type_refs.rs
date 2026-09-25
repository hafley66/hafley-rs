//! Rust type text resolution beside SCM's annotation-reference projection.

use crate::read::types::TypeEdgeKind;

pub use hafley_scm::lang::rust::{path_name, primary_type};
pub use hafley_scm::lang::rust::{collect_path_args, type_refs};

pub fn type_probe_key(name: &str, kind: TypeEdgeKind) -> (Option<&str>, &str) {
    // A Variant candidate's `to` is v5's synthetic `Enum::Variant` text, not a
    // path: text dsts stay text.
    match name.rsplit_once("::") {
        Some((qualifier, trailing)) if kind != TypeEdgeKind::Variant => (Some(qualifier), trailing),
        _ => (None, name),
    }
}
