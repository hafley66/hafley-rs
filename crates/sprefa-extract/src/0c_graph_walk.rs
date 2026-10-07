//! `graph --slow --from/--call-path` on a Rust seed: rust-analyzer's demand walk
//! from the seed bodies; no whole-project store is built for the question.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sprefa_extract::lang::rust_checker::{demand_walk, EdgeKind, WalkQuestion};
use sprefa_extract::FlatFact;

use super::{named_starts, nodes, paths, Arm, Deadline, Lines, PlaneEdge};
use crate::cli::GraphArgs;
use crate::sqlite::REACH_DEPTH_CAP;

/// Languages without call edges: they never make a bare-NAME corpus mixed.
const CALLLESS: &[&str] = &["markdown", "data", "fallback"];

/// The eager slow tier's workspace-load budget.
const LOAD_BUDGET: Duration = Duration::from_secs(900);

/// The anchor when the walk answers: `--slow` `--from`/`--call-path` on the
/// working tree with no SCIP index and no published store, seeded in Rust.
pub(super) fn rust_anchor<'a>(arm: &Arm<'a>, cli: &GraphArgs, files: &[PathBuf]) -> Option<&'a str> {
    if !cli.slow || cli.at.is_some() || cli.scip_index.is_some() || cli.sqlite.is_some() {
        return None;
    }
    let anchor = match arm {
        Arm::From(anchor) | Arm::CallPath(anchor) => *anchor,
        _ => return None,
    };
    let rust = |path: &Path| path.extension().is_some_and(|extension| extension == "rs");
    let seeded = match anchor.split_once('#') {
        Some((path, _)) => rust(Path::new(path)),
        None => {
            files.iter().any(|path| rust(path))
                && files.iter().all(|path| {
                    rust(path)
                        || sprefa_extract::lang::source_for(&path.to_string_lossy())
                            .is_none_or(|source| CALLLESS.contains(&source.name()))
                })
        }
    };
    seeded.then_some(anchor)
}

pub(super) fn walk_rows(
    arm: &Arm<'_>,
    cli: &GraphArgs,
    files: &[PathBuf],
    anchor: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    let root = crate::inputs::root(&cli.inputs);
    let io_root = sprefa_extract::io_path(&root);
    let root = std::fs::canonicalize(&io_root).unwrap_or(io_root);
    let files: Vec<(String, PathBuf)> = files
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .map(|path| {
            let io_path = sprefa_extract::io_path(path);
            let absolute = std::fs::canonicalize(&io_path).unwrap_or(io_path);
            (path.to_string_lossy().into_owned(), absolute)
        })
        .collect();
    let (path, name) = match anchor.split_once('#') {
        Some((path, name)) => (Some(Path::new(path)), name),
        None => (None, anchor),
    };
    let seeds: Vec<(String, String)> = files
        .iter()
        .filter(|(supplied, _)| path.is_none_or(|path| super::anchor::anchor_path_matches(path, supplied)))
        .map(|(supplied, _)| (supplied.clone(), name.to_string()))
        .collect();
    // `--timeout` covers the walk, as it covers the eager tier's query and not its load;
    // a walk past it fails after `within`'s own clock, so the exit is 3.
    let answer = crate::deadline::within(None, Some(cli.timeout), "graph", |_| {
        let question = WalkQuestion {
            root: &root,
            files: &files,
            seeds: &seeds,
            max_depth: Some(REACH_DEPTH_CAP),
            timeout: Some(Duration::from_secs(cli.timeout)),
            budget: LOAD_BUDGET,
        };
        demand_walk(&question).map_err(|error| error.to_string().into())
    })?;
    for path in &answer.unowned_seeds {
        crate::ops::print_diagnostic(format_args!(
            "ryi slow: tier.rust-analyzer declined {path}: owns no module in the loaded crate graph (cfg-gated, or outside every crate root)"
        ));
    }
    let count = |kind: EdgeKind| answer.edges.iter().filter(|edge| edge.kind == kind).count();
    crate::ops::print_diagnostic(format_args!(
        "demand walk: {} bodies inferred, {} call, {} method, {} passed, {} trait_impl, {} extern edges",
        answer.bodies_inferred,
        count(EdgeKind::Call),
        count(EdgeKind::Method),
        count(EdgeKind::Passed),
        count(EdgeKind::TraitImpl),
        answer.edges.iter().filter(|edge| edge.to.path.is_empty()).count(),
    ));
    // An extern target is no graph node, as in the eager store's resolved edges.
    let edges: Vec<PlaneEdge> = answer
        .edges
        .into_iter()
        .filter(|edge| !edge.to.path.is_empty())
        .enumerate()
        .map(|(row, edge)| PlaneEdge {
            row: row as u64 + 1,
            src: (edge.from.path, Some(edge.from.name)),
            dst: (edge.to.path, Some(edge.to.name)),
            grade: "+".to_string(),
            dst_start: Some(edge.to.start),
        })
        .collect();
    let walked = Deadline { at: None };
    let mut lines = Lines {
        root: cli.inputs.root.clone(),
        source_root: None,
        tables: Default::default(),
    };
    match arm {
        Arm::From(_) => nodes(&edges, anchor, &walked, &mut lines),
        _ => paths(&edges, "call", |edges| Ok(named_starts(edges, anchor)), &walked),
    }
}
