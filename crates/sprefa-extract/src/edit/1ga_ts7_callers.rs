//! `graph --slow --callers NAME` on TypeScript: the checker is asked only at the
//! call sites (and JSX attributes) whose written name is NAME, in the files that
//! hold them. The text match picks the questions; each edge is the checker's answer.

use std::path::Path;

use super::checker_edges::{CheckerDefs, CheckerEdge};
use super::ts7_resolve::References;
use crate::{FamilyMask, RawProjectFact};

/// `candidates`: the supplied paths holding a call site written `name`.
/// `supplied`: every path of the question; a destination outside it is no edge.
pub fn callers(
    root: &Path,
    name: &str,
    candidates: &[&str],
    supplied: &[&str],
    defs: &CheckerDefs,
) -> Result<Vec<CheckerEdge>, String> {
    let _checker_span = crate::trace::tracked(tracing::info_span!("typescript.checker")).entered();
    let mut references = References::default();
    references.demand = Some(name.to_string());
    let mask = FamilyMask { cst: true, types: false, call: true, df: false, data: false };
    for path in candidates {
        let content = std::fs::read(crate::io_path(Path::new(path)))
            .map_err(|error| format!("read {path}: {error}"))?;
        let blob = crate::content_id_of(&content);
        let Some(output) = crate::dispatch(path, &content, mask) else { continue };
        crate::flatten_each(&output, None, &mut |fact| {
            references.capture(&RawProjectFact { path, content_id: &blob, content: &content, fact });
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
    }
    references.close();
    if !references.has_questions() {
        return Ok(Vec::new());
    }
    // JSX attribute rows land here; a callers question reads only the edges.
    let mut attribute_rows = Vec::new();
    references.ask(root, supplied, defs, &mut attribute_rows)
}
