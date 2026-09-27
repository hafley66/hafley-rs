//! Parse and query one Rust file for the SCM++ read front-end.

use std::sync::LazyLock;

use tree_sitter::{Language, Tree};

use super::{rust_combined_query, RUST_CALL_QUERY};

pub struct RustFastFile {
    tree: Tree,
    arena: crate::MatchArena,
}

impl RustFastFile {
    /// Parse the source once and execute the combined Rust call and fast-row
    /// query once. The returned tree and arena back all per-file projections.
    pub fn extract(path: &str, source: &[u8]) -> Option<Self> {
        std::str::from_utf8(source).ok()?;
        let language = Language::new(tree_sitter_rust::LANGUAGE);
        let tree = {
            #[cfg(feature = "read")]
            let parse_span = crate::read::trace::parse_span("rust", "tree-sitter");
            #[cfg(feature = "read")]
            let _parse_guard = parse_span.enter();
            crate::cst::parse(&language, source)?
        };
        let query = fast_file_query();
        let mut arena = crate::MatchArena::default();
        {
            #[cfg(feature = "read")]
            let query_span =
                crate::read::trace::phase_span("rust", crate::read::trace::Phase::Query);
            #[cfg(feature = "read")]
            let _query_guard = query_span.enter();
            crate::run(query, path, source, &tree, u32::MAX, &mut arena)
                .expect("rust combined query stays within the engine match limit");
            #[cfg(feature = "read")]
            crate::read::trace::record_phase(&query_span, source.len() as u64, 0, 1);
        }
        Some(Self { tree, arena })
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn arena(&self) -> &crate::MatchArena {
        &self.arena
    }
}

pub fn fast_file_query() -> &'static crate::QueryExt {
    static QUERY: LazyLock<crate::QueryExt> = LazyLock::new(|| {
        let language = Language::new(tree_sitter_rust::LANGUAGE);
        crate::build(&language, &rust_combined_query()).expect("rust combined query builds")
    });
    &QUERY
}

pub fn call_query() -> &'static crate::QueryExt {
    static QUERY: LazyLock<crate::QueryExt> = LazyLock::new(|| {
        let language = Language::new(tree_sitter_rust::LANGUAGE);
        crate::build(&language, RUST_CALL_QUERY).expect("rust call query builds")
    });
    &QUERY
}
