mod pipeline;
mod types;
mod walk;

pub use types::*;

use tree_sitter::{Language, Query, Tree};

use pipeline::split_predicates_into_kind_queries as split;
use pipeline::{build_query_ext as build, run_over_file_tree as run};

pub fn build(language: &Language, scm: &str) -> Result<QueryExt, QueryExtError> {
    let user = Query::new(language, scm).map_err(QueryExtError::Parse)?;
    let (predicates, kind_names) = split::read_and_parse_predicates(&user)?;
    let kinds = build::query_new_per_kind(language, &kind_names)?;
    let names = build::intern_names(&user);
    Ok(QueryExt {
        user,
        kinds,
        predicates,
        names,
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
    let kind_ids = run::kind_cursors_into_sorted_ids(q, tree, src);
    let file = arena.files.len() as u16;
    arena.files.push(path.into());
    run::user_cursor_into_arena(q, tree, src, limit, &kind_ids, file, arena)
}
