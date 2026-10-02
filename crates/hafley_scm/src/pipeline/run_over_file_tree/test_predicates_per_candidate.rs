use std::collections::HashSet;

use tree_sitter::{QueryMatch, Tree};

use crate::scmpp::MatchKey;
use crate::types::{QueryExt, QueryExtError};

/// Accepted level-0 matches per routed pattern, evaluated on the first candidate of that pattern.
pub type Accepted = Vec<Option<HashSet<MatchKey>>>;

/// A pattern without relation predicates holds as tree-sitter matched it; a routed pattern holds
/// when its captures name a match the scm++ SQL kept.
pub fn holds_for_candidate(
    q: &QueryExt,
    found: &QueryMatch,
    tree: &Tree,
    src: &[u8],
    accepted: &mut Accepted,
) -> Result<bool, QueryExtError> {
    let Ok(routed) = q
        .scmpp
        .binary_search_by_key(&(found.pattern_index as u16), |(pattern, _)| *pattern)
    else {
        return Ok(true);
    };
    if accepted[routed].is_none() {
        accepted[routed] = Some(evaluate(&q.scmpp[routed].1, tree, src)?);
    }
    let mut key: MatchKey = found
        .captures()
        .iter()
        .map(|capture| {
            (
                q.names[capture.index as usize].clone(),
                capture.node.start_byte() as u32,
                capture.node.end_byte() as u32,
            )
        })
        .collect();
    key.sort_unstable();
    Ok(accepted[routed]
        .as_ref()
        .is_some_and(|keys| keys.contains(&key)))
}

#[cfg(feature = "shared")]
fn evaluate(
    compiled: &crate::scmpp::Compiled,
    tree: &Tree,
    src: &[u8],
) -> Result<HashSet<MatchKey>, QueryExtError> {
    crate::scmpp::accepted(compiled, tree, src).map_err(QueryExtError::Scmpp)
}

#[cfg(not(feature = "shared"))]
fn evaluate(
    _: &crate::scmpp::Compiled,
    _: &Tree,
    _: &[u8],
) -> Result<HashSet<MatchKey>, QueryExtError> {
    Err(QueryExtError::Scmpp(crate::scmpp::ScmppError::Unsupported(
        "relation predicates evaluate in SQLite; build hafley_scm with feature shared".into(),
    )))
}
