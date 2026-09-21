mod types;
pub use types::*;

use tree_sitter::{Language, Node, Query, Tree};

pub fn build(language: &Language, scm: &str) -> Result<QueryExt, QueryExtError> {
    let user = Query::new(language, scm).map_err(QueryExtError::Parse)?;
    let (predicates, kind_names) = split::read_and_parse_predicates(&user)?;
    let kinds = build::query_new_per_kind(language, &kind_names)?;
    let names = build::intern_names(&user);
    Ok(QueryExt { user, kinds, predicates, names })
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

/// pipeline/split_predicates_into_kind_queries
mod split {
    use super::*;

    /// ts: `user.general_predicates(i)` per pattern; unknown operator or bad arity is an error.
    pub fn read_and_parse_predicates(_user: &Query) -> Result<(Vec<Predicate>, Vec<Box<str>>), QueryExtError> {
        todo!("has-ancestor? has? with not- prefix, third arg neighbor|end, kind strings deduped")
    }

    /// `"function_item"` -> `(function_item) @_`
    pub fn mint_kind_query_text(_kind: &str) -> String {
        todo!()
    }
}

/// pipeline/build_query_ext
mod build {
    use super::*;

    /// ts: one `Query::new` per deduped kind, built once per language.
    pub fn query_new_per_kind(_language: &Language, _kinds: &[Box<str>]) -> Result<Vec<Query>, QueryExtError> {
        todo!()
    }

    pub fn intern_names(_user: &Query) -> Vec<Box<str>> {
        todo!("user.capture_names() as Box<str>")
    }
}

/// pipeline/run_over_file_tree
mod run {
    use super::*;

    /// ts: one cursor per kind query, node ids collected and sorted before any candidate is tested.
    pub fn kind_cursors_into_sorted_ids(_q: &QueryExt, _tree: &Tree, _src: &[u8]) -> Vec<Vec<u32>> {
        todo!()
    }

    /// ts: user cursor with `set_match_limit`; each kept match appends spans then one row.
    pub fn user_cursor_into_arena(
        _q: &QueryExt,
        _tree: &Tree,
        _src: &[u8],
        _limit: u32,
        _kind_ids: &[Vec<u32>],
        _file: u16,
        _arena: &mut MatchArena,
    ) -> Result<(), QueryExtError> {
        todo!("keep iff walk::holds != negated; did_exceed_match_limit -> MatchLimit")
    }
}

/// walk: iterative only, hit = `kind_ids.binary_search(&(n.id() as u32)).is_ok()`
mod walk {
    use super::*;

    pub fn holds(_p: &Predicate, _node: Node, _kind_ids: &[u32]) -> bool {
        todo!("Ancestor: parent() once or to root; Descendant: children once or TreeCursor preorder")
    }
}
