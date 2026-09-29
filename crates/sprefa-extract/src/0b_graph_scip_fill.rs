//! Add SCIP call rows only when target call sites remain unanswered.

use std::path::{Path, PathBuf};

use sprefa_extract::{slow_project, FlatFact};

pub(super) fn fill_callers(
    facts: &mut Vec<FlatFact>,
    paths: &[PathBuf],
    root: &Path,
    name: &str,
    sites: &[(String, u32, u32, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let uncovered = sites.iter().any(|(path, start, end, _)| {
        !facts.iter().any(|fact| {
            matches!(fact, FlatFact::ResolvedEdge {
                caller_path, callee_name, caller_site_start, caller_site_end, ..
            } if caller_path == path && callee_name.as_deref() == Some(name)
                && ((*caller_site_start <= *start && *end <= *caller_site_end)
                    || (*start <= *caller_site_start && *caller_site_end <= *end)))
        })
    });
    if !uncovered {
        return Ok(());
    }
    let scip: Vec<FlatFact> = slow_project(paths, root, None, false)?
        .into_iter()
        .filter(|fact| {
            matches!(fact, FlatFact::ResolvedEdge { callee_name, .. }
                if callee_name.as_deref() == Some(name))
        })
        .collect();
    for fact in facts.iter_mut() {
        let FlatFact::ResolvedEdge {
            caller_path,
            callee_path,
            caller_site_start,
            caller_site_end,
            callee_start,
            callee_end,
            resolution_origin,
            ..
        } = fact
        else {
            continue;
        };
        if resolution_origin != "checker" {
            continue;
        }
        if let Some(FlatFact::ResolvedEdge {
            callee_start: scip_start,
            callee_end: scip_end,
            ..
        }) = scip.iter().find(|row| {
            matches!(row, FlatFact::ResolvedEdge {
            caller_path: source, callee_path: target,
            caller_site_start: start, caller_site_end: end, ..
        } if source == caller_path && target == callee_path
            && *start <= *caller_site_end && *caller_site_start <= *end)
        }) {
            *callee_start = *scip_start;
            *callee_end = *scip_end;
        }
    }
    facts.retain(|fact| {
        let FlatFact::ResolvedEdge {
            resolution_origin, ..
        } = fact
        else {
            return true;
        };
        resolution_origin == "checker"
    });
    facts.extend(scip);
    Ok(())
}
