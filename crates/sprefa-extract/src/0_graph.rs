use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords};

pub struct GraphCx {
    facts: Vec<FlatFact>,
    visited: BTreeSet<(String, String)>,
}

impl GraphCx {
    pub fn load(paths: &[PathBuf], arms: ResolveArms) -> Result<GraphCx, Box<dyn std::error::Error>> {
        let request = ResolveRequest {
            paths,
            arms,
            scip: ScipMode::Off,
            project_root: None,
            scip_records: ScipRecords::default(),
            occurrence_text: false,
            rust_checker: None,
            ts_checker: None,
            go_checker: None,
            witness: false,
        };
        let facts = resolve_project(&request)?;
        Ok(GraphCx {
            facts,
            visited: BTreeSet::new(),
        })
    }

    fn resolved_edges(&self) -> impl Iterator<Item = &FlatFact> {
        self.facts
            .iter()
            .filter(|fact| matches!(fact, FlatFact::ResolvedEdge { .. }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    Plus,
    Tilde,
    Minus,
}

impl Grade {
    pub fn from_origin(origin: &str) -> Grade {
        match origin {
            "module_plane" => Grade::Plus,
            "unresolved" => Grade::Minus,
            _ => Grade::Tilde,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Grade::Plus => "+",
            Grade::Tilde => "~",
            Grade::Minus => "-",
        }
    }
}

#[derive(Default)]
struct GradeSplit {
    plus: u32,
    tilde: u32,
    minus: u32,
}

impl GradeSplit {
    fn bump(&mut self, grade: Grade) {
        match grade {
            Grade::Plus => self.plus += 1,
            Grade::Tilde => self.tilde += 1,
            Grade::Minus => self.minus += 1,
        }
    }
}

pub fn run_callers(cx: &GraphCx, name: &str) -> Vec<FlatFact> {
    let mut edges = Vec::new();
    for fact in cx.resolved_edges() {
        let FlatFact::ResolvedEdge {
            caller_path,
            caller_name,
            callee_path,
            callee_name,
            kind,
            resolution_origin,
            ..
        } = fact
        else {
            unreachable!();
        };
        if callee_name.as_deref() == Some(name) {
            let grade = Grade::from_origin(resolution_origin);
            edges.push(FlatFact::GraphEdge {
                from_path: callee_path.clone(),
                from_name: callee_name.clone(),
                to_path: caller_path.clone(),
                to_name: caller_name.clone(),
                kind: kind.clone(),
                grade: grade.as_str().to_string(),
            });
        }
    }
    edges.sort_by_key(|edge| serde_json::to_string(edge).expect("graph edge serializes"));
    edges
}

fn emit_edges(edges: &[FlatFact]) -> Result<(), Box<dyn std::error::Error>> {
    for edge in edges {
        println!("{}", serde_json::to_string(edge)?);
    }
    Ok(())
}

fn emit_summary_line(edges: &[FlatFact]) {
    let mut split = GradeSplit::default();
    for edge in edges {
        if let FlatFact::GraphEdge { grade, .. } = edge {
            split.bump(match grade.as_str() {
                "+" => Grade::Plus,
                "-" => Grade::Minus,
                _ => Grade::Tilde,
            });
        }
    }
    eprintln!(
        "{} edges: {} +, {} ~, {} -",
        edges.len(), split.plus, split.tilde, split.minus
    );
}

fn expand_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    let mut pending = paths.to_vec();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if path.extension().is_some_and(|extension| extension == "ts") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = arguments.collect::<Vec<_>>().into_iter();
    let mut caller_name = None;
    let mut paths = Vec::new();
    let mut json = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--callers" => caller_name = args.next(),
            "--json" => json = true,
            _ if argument.starts_with('-') => {
                return Err(format!("unknown graph argument {argument}").into())
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }
    let caller_name = caller_name.ok_or("graph requires --callers NAME")?;
    if paths.is_empty() {
        return Err("graph requires PATH".into());
    }
    let paths = expand_paths(&paths)?;
    let cx = GraphCx::load(&paths, ResolveArms { call: true, ..ResolveArms::default() })?;
    let edges = run_callers(&cx, &caller_name);
    emit_edges(&edges)?;
    if !json {
        emit_summary_line(&edges);
    }
    Ok(())
}
