extern crate self as hafley_scm;

mod pipeline;
mod types;

#[cfg(feature = "rust_syn")]
pub mod lang;

pub mod atoms;
pub mod cst;
#[cfg(feature = "shared")]
pub mod read;
pub mod scmpp;
pub mod span;
pub use types::*;

use tree_sitter::{Language, Query, Tree};

use pipeline::split_predicates_into_kind_queries as split;
use pipeline::{build_query_ext as build, run_over_file_tree as run};

/// Relation predicates route through `scmpp`: their patterns compile there, the rest is plain tree-sitter.
pub fn build(language: &Language, scm: &str) -> Result<QueryExt, QueryExtError> {
    let routed = scmpp::route(language, scm).map_err(QueryExtError::Scmpp)?;
    let user = Query::new(language, &routed.text).map_err(QueryExtError::Parse)?;
    let scmpp = build::pair_routed_patterns(&user, routed.items);
    let (emits, relations, fields, emit_literals) = split::read_and_parse_predicates(&user)?;
    let names = build::intern_names(&user);
    Ok(QueryExt {
        user,
        scmpp,
        names,
        emits,
        relations,
        fields,
        emit_literals,
    })
}

pub fn run(
    q: &QueryExt,
    path: &str,
    src: &[u8],
    tree: &Tree,
    limit: u32,
    arena: &mut MatchArena,
) -> Result<(), QueryExtError> {
    let file = arena.files.len() as u16;
    arena.files.push(path.into());
    run::user_cursor_into_arena(q, tree, src, limit, file, arena)
}
