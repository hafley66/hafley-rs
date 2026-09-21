//! `ryi graph`: one resolve pass over a corpus, then a question asked of it.
//! No language is named here: the path roster answers which files exist, and
//! the resolve arms answer what they mean.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use clap::{ArgGroup, Parser};
use sprefa_extract::lang::source_for;
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

/// Every file under `paths` the roster claims. No suffix is spelled here, so a
/// language the roster gains is walked by this verb the same day.
fn expand_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    let mut pending = paths.to_vec();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if source_for(&path.to_string_lossy()).is_some() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// The out-of-scope list the help text states, so a caller reads it before the
/// run rather than after.
const SCOPE: &str = "Exactly one of --callers, --uses and --from is required. Out of scope, each \
                     its own issue: a persistent cross-run graph index (dl8 owns it), a \
                     maintained liveness or dead-code view, and grading a reach hop by anything \
                     but the edge that discovered it.";

#[derive(Parser)]
#[command(
    name = "ryi graph",
    about = "ask one question of the resolved call and type graph of a corpus",
    after_help = SCOPE
)]
#[command(group(ArgGroup::new("arm").required(true).args(["callers", "uses", "from"])))]
pub struct GraphCli {
    /// Files and directories. A directory is walked; every path the language
    /// roster claims is read, and nothing else.
    #[arg(required = true, value_name = "PATH")]
    paths: Vec<PathBuf>,
    /// Who calls NAME: one row per resolved call edge landing on it.
    #[arg(long, value_name = "NAME")]
    callers: Option<String>,
    /// Who references the type NAME: one row per referencing declaration.
    #[arg(long, value_name = "NAME")]
    uses: Option<String>,
    /// What NAME reaches along resolved call edges, transitively.
    #[arg(long, value_name = "NAME")]
    from: Option<String>,
    /// Keep the fact store in this directory instead of memory.
    #[arg(long, value_name = "DIR")]
    state: Option<PathBuf>,
    /// Drop the stderr summary line; stdout is JSONL either way.
    #[arg(long)]
    json: bool,
}

pub fn run<I>(args: I) -> Result<(), Box<dyn std::error::Error>>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    // `exit` rather than a returned error: `--help` is an `Err` to clap, and
    // only clap's own exit prints it to stdout with status 0.
    let cli = match GraphCli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let paths = expand_paths(&cli.paths)?;
    let Some(name) = cli.callers.as_deref() else {
        return Err("--uses and --from arrive in the next step".into());
    };
    let cx = GraphCx::load(
        &paths,
        ResolveArms {
            call: true,
            ..ResolveArms::default()
        },
    )?;
    let _ = &cli.state;
    let edges = run_callers(&cx, name);
    emit_edges(&edges)?;
    if !cli.json {
        emit_summary_line(&edges);
    }
    Ok(())
}
