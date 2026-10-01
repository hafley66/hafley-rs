//! The go CHECKER tier: `go/types`, driven by a sidecar this crate builds,
//! answers the DESTINATION of a reference this crate's parse found; caller,
//! site spans and drops stay ours.
//!
//! The join runs as a project post-pass rather than inside the go arm, so one
//! tier reaches both go families without the arm carrying a second resolution order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{answer_of, CALL_FACETS, TYPE_FACETS};
pub use super::{
    CheckerAnswer as GoCheckerAnswer, CheckerAnswers as GoCheckerAnswers,
    CheckerRef as GoCheckerRef,
};
use crate::read::shape::NodeRef;
use crate::read::tsi::stamp_digests;
use crate::read::types::{
    CallEdgeKind, CallF, ContentId, DefIndex, ProjectEdge, ResolutionOrigin, RyiOutput, TypeF,
};
use hafley_scm::span::Span;

type Bound = super::CheckerBound;

/// Why the tier could not run. Every one falls back to the syntax leg.
#[derive(Debug)]
pub enum GoCheckerError {
    NotBuilt,
    /// `go` is not on PATH, or the sidecar could not be staged or compiled.
    NoDriver(String),
    /// The sidecar ran and failed; the string is its last stderr line.
    Failed(String),
    Budget(u64),
}

impl std::fmt::Display for GoCheckerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBuilt => write!(
                f,
                "the go checker tier needs --features go-checker; falling back to the syntax leg"
            ),
            Self::NoDriver(detail) => write!(f, "no go driver: {detail}"),
            Self::Failed(detail) => write!(f, "the driver failed: {detail}"),
            Self::Budget(secs) => write!(f, "the driver exceeded {secs}s"),
        }
    }
}

/// Every answer joined ONCE to a `(blob, def span)` at build time; per-file
/// lists sorted by start, so a site lookup is a range scan, not a corpus walk.
#[derive(Default)]
pub struct GoCheckerIndex {
    calls: HashMap<String, Vec<Bound>>,
    /// A TypeF candidate carries no reference span, so the type plane keys on
    /// (file, name AS WRITTEN); a name one file resolves two ways binds nothing.
    types: HashMap<String, HashMap<String, Option<GoCheckerAnswer>>>,
    /// The name behind each corpus def coordinate. The type post-pass reads a
    /// syntax edge's target name off this instead of re-walking the def index.
    def_names: HashMap<(ContentId, u32, u32), String>,
    tsi: Vec<crate::read::tsi::FactOut>,
    coverage: Vec<crate::read::tsi::CoverageClaim>,
    /// Answers naming a corpus file whose parse minted no def there; they fall
    /// back to the syntax leg, so this is the tier's own miss count.
    pub unjoined: usize,
    /// References the checker resolved outside the corpus.
    pub external: usize,
    pub load: Duration,
    pub walk: Duration,
    pub files_answered: usize,
}

impl GoCheckerIndex {
    /// An answer naming a file outside the resolve universe, or a def
    /// coordinate the parse never minted, is dropped: the syntax leg answers.
    pub fn build(
        answers: GoCheckerAnswers,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> GoCheckerIndex {
        let blob_of: HashMap<&str, &ContentId> = corpus
            .iter()
            .map(|(path, blob)| (path.as_str(), blob))
            .collect();
        let mut def_names: HashMap<(ContentId, u32, u32), String> = HashMap::new();
        for (name, sites) in &defs.map {
            for site in sites {
                def_names
                    .entry((site.blob.clone(), site.span.start, site.span.end()))
                    .or_insert_with(|| name.clone());
            }
        }
        let mut index = GoCheckerIndex {
            load: answers.load,
            walk: answers.walk,
            files_answered: answers.files_answered,
            tsi: stamp_digests(answers.tsi, corpus),
            def_names,
            coverage: answers
                .coverage
                .into_iter()
                .map(
                    |(relation, complete, diagnostic)| crate::read::tsi::CoverageClaim {
                        relation,
                        complete,
                        diagnostic,
                    },
                )
                .collect(),
            ..GoCheckerIndex::default()
        };
        for (path, refs) in answers.calls {
            let mut bounds: Vec<Bound> = Vec::with_capacity(refs.len());
            for reference in refs {
                match answer_of(
                    (
                        &reference.dst_path,
                        &reference.dst_name,
                        reference.dst_offset,
                    ),
                    CALL_FACETS,
                    &blob_of,
                    defs,
                ) {
                    Some(answer) => {
                        index.external += matches!(answer, GoCheckerAnswer::External(_)) as usize;
                        bounds.push(Bound {
                            start: reference.start,
                            end: reference.end,
                            name: reference.name,
                            answer,
                        });
                    }
                    None => index.unjoined += 1,
                }
            }
            bounds.sort_by_key(|bound| (bound.start, bound.end));
            index.calls.insert(path, bounds);
        }
        for (path, refs) in answers.types {
            let mut by_name: HashMap<String, Option<GoCheckerAnswer>> = HashMap::new();
            for reference in refs {
                let Some(answer) = answer_of(
                    (
                        &reference.dst_path,
                        &reference.dst_name,
                        reference.dst_offset,
                    ),
                    TYPE_FACETS,
                    &blob_of,
                    defs,
                ) else {
                    index.unjoined += 1;
                    continue;
                };
                index.external += matches!(answer, GoCheckerAnswer::External(_)) as usize;
                match by_name.entry(reference.name) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(Some(answer));
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        if slot.get().as_ref() != Some(&answer) {
                            slot.insert(None);
                        }
                    }
                }
            }
            index.types.insert(path, by_name);
        }
        index
    }

    /// Whether the tier answered this file at all. A file it never saw keeps
    /// every syntax edge untouched.
    pub fn knows(&self, path: &str) -> bool {
        self.calls.contains_key(path) || self.types.contains_key(path)
    }

    /// A site span covers the whole callee expression (`a.b.c(x)`): the answer
    /// is the RIGHTMOST inside it carrying the name.
    pub fn call_at(&self, path: &str, site: Span, callee: &str) -> Option<GoCheckerAnswer> {
        let bounds = self.calls.get(path)?;
        let end = site.end();
        bounds
            .iter()
            .filter(|bound| bound.start >= site.start && bound.end <= end && bound.name == callee)
            .next_back()
            .map(|bound| bound.answer.clone())
    }

    /// `name` is the candidate's `to` AS WRITTEN, which is the text the sidecar
    /// keys on too.
    pub fn type_at(&self, path: &str, name: &str) -> Option<GoCheckerAnswer> {
        self.types.get(path)?.get(name)?.clone()
    }

    pub fn semantic_rows(&self) -> &[crate::read::tsi::FactOut] {
        &self.tsi
    }

    pub fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        &self.coverage
    }
}

impl crate::read::tsi::SemanticRows for GoCheckerIndex {
    fn facts(&self) -> &[crate::read::tsi::FactOut] {
        self.semantic_rows()
    }

    fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        GoCheckerIndex::coverage(self)
    }
}

/// The tier's answers folded into one file's already-resolved call edges. A
/// site the checker names in the corpus takes the checker's coordinate and
/// leg; a site it names OUTSIDE loses its syntax edge, since the checker
/// knowing the target is off-corpus is knowledge the name match cannot beat.
pub fn apply_calls(
    index: &GoCheckerIndex,
    path: &str,
    output: &RyiOutput,
    edges: &mut Vec<ProjectEdge<CallF>>,
) {
    if !index.knows(path) {
        return;
    }
    let Some(call) = output.call.as_ref() else {
        return;
    };
    let mut answered: HashSet<(u32, u32)> = HashSet::new();
    edges.retain_mut(|edge| {
        let Some(site) = edge.call_site else {
            return true;
        };
        let Some(callee) = site_callee(call, output, site) else {
            return true;
        };
        match index.call_at(path, site, callee) {
            Some(GoCheckerAnswer::Corpus(blob, span)) => {
                answered.insert((site.start, site.end()));
                edge.dst_blob = blob;
                edge.dst_span = span;
                edge.kind = CallEdgeKind::CheckerResolve;
                edge.origin = ResolutionOrigin::Checker;
                if !edge.witnesses.is_empty() {
                    edge.witnesses.push(ResolutionOrigin::Checker);
                }
                true
            }
            Some(GoCheckerAnswer::External(_)) => false,
            None => true,
        }
    });
    // A site the syntax leg dropped and the checker answered is the tier's
    // whole recall gain, so it mints an edge of its own.
    for site in &call.aux.sites {
        if answered.contains(&(site.span.start, site.span.end())) {
            continue;
        }
        let callee = output.strings.lookup(site.callee);
        let Some(GoCheckerAnswer::Corpus(blob, span)) = index.call_at(path, site.span, callee)
        else {
            continue;
        };
        let Some(src) = crate::read::types::covering_def(call, site.span) else {
            continue;
        };
        edges.push(
            ProjectEdge::new(
                src,
                blob,
                span,
                CallEdgeKind::CheckerResolve,
                ResolutionOrigin::Checker,
            )
            .with_call_site(site.span),
        );
    }
}

/// The type twin. The type plane is name-keyed on both sides, so a syntax edge
/// whose target carries a name the checker also answered for this file is the
/// one the checker's own edge replaces.
pub fn apply_types(
    index: &GoCheckerIndex,
    path: &str,
    output: &RyiOutput,
    edges: &mut Vec<ProjectEdge<TypeF>>,
) {
    if !index.knows(path) {
        return;
    }
    let Some(types) = output.types.as_ref() else {
        return;
    };
    let mut minted: Vec<ProjectEdge<TypeF>> = Vec::new();
    let mut replaced: HashSet<String> = HashSet::new();
    for candidate in &types.aux.candidates {
        let name = output.strings.lookup(candidate.to);
        match index.type_at(path, name) {
            Some(GoCheckerAnswer::Corpus(blob, span)) => {
                replaced.insert(name.to_string());
                let Some(src) = types
                    .nodes
                    .iter()
                    .position(|node| node.span == candidate.owner)
                else {
                    continue;
                };
                minted.push(ProjectEdge::new(
                    NodeRef(src as u32),
                    blob,
                    span,
                    candidate.kind,
                    ResolutionOrigin::Checker,
                ));
            }
            Some(GoCheckerAnswer::External(_)) => {
                replaced.insert(name.to_string());
            }
            None => {}
        }
    }
    if replaced.is_empty() {
        return;
    }
    edges.retain(|edge| {
        match index.def_names.get(&(
            edge.dst_blob.clone(),
            edge.dst_span.start,
            edge.dst_span.end(),
        )) {
            Some(name) => !replaced.contains(name),
            None => true,
        }
    });
    // One (src, kind, target) per row: the same name written twice in one
    // signature is one edge, the way the syntax leg's own candidates fold.
    let mut seen: HashSet<(u32, String, u32, u32, &'static str)> = HashSet::new();
    for edge in minted {
        let key = (
            edge.src.0,
            edge.dst_blob.to_string(),
            edge.dst_span.start,
            edge.dst_span.end(),
            edge.kind.as_str(),
        );
        if seen.insert(key) {
            edges.push(edge);
        }
    }
}

/// The callee spelling at one call site, which is the name the sidecar wrote
/// beside its answer.
fn site_callee<'a>(
    call: &crate::read::types::FamilyBundle<CallF>,
    output: &'a RyiOutput,
    site: Span,
) -> Option<&'a str> {
    call.aux
        .sites
        .iter()
        .find(|candidate| candidate.span == site)
        .map(|candidate| output.strings.lookup(candidate.callee))
}

/// The declaration identifier's offset picks between several defs of one name
/// in one file; a lone def of the name binds without it.
/// Run the checker over `root` and answer every reference in `files`
/// (supplied path, absolute path).
#[cfg(not(feature = "go-checker"))]
pub fn answer(
    _root: &Path,
    _files: &[(String, PathBuf)],
    _tsi: bool,
) -> Result<GoCheckerAnswers, GoCheckerError> {
    Err(GoCheckerError::NotBuilt)
}

/// The sidecar, embedded rather than installed: a tier that needs a separate
/// `go install` to answer is a tier that silently does not run.
#[cfg(feature = "go-checker")]
const DRIVER_MAIN: &str = include_str!("../../../../sprefa-extract/tools/go_checker/main.go");
#[cfg(feature = "go-checker")]
const DRIVER_MOD: &str = include_str!("../../../../sprefa-extract/tools/go_checker/go.mod");
#[cfg(feature = "go-checker")]
const DRIVER_SUM: &str = include_str!("../../../../sprefa-extract/tools/go_checker/go.sum");

/// One driver row `[relation, arg, ...]` into a fact. A row the registry does
/// not know, or an argument it cannot decode, stops the tier.
#[cfg(feature = "go-checker")]
fn into_fact(row: Vec<serde_json::Value>) -> Result<crate::read::tsi::FactOut, GoCheckerError> {
    let mut parts = row.into_iter();
    let relation = parts
        .next()
        .and_then(|head| head.as_str().map(str::to_string))
        .ok_or_else(|| GoCheckerError::Failed("a tsi row opens with its relation".to_string()))?;
    let args: Vec<crate::read::tsi::Arg> = parts
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|err| GoCheckerError::Failed(format!("{relation}: {err}")))?;
    crate::read::tsi::registry::check(&relation, &args)
        .map_err(|detail| GoCheckerError::Failed(format!("{relation}: {detail}")))?;
    Ok(crate::read::tsi::FactOut {
        fact: 0,
        relation,
        args,
    })
}

/// The sidecar compiled ONCE per distinct source, keyed by its own digest, so
/// a run pays `go build` only the first time it sees this driver. `go run`
/// re-links per invocation, which is the cost this cache exists to skip.
#[cfg(feature = "go-checker")]
fn staged_binary() -> Result<PathBuf, GoCheckerError> {
    use crate::read::scip_ensure::{run_capped, Capped};

    let stage = GoCheckerError::NoDriver;
    let digest =
        ContentId::blake3(format!("{DRIVER_MAIN}\u{0}{DRIVER_MOD}\u{0}{DRIVER_SUM}").as_bytes())
            .to_string();
    let short: String = digest.chars().rev().take(16).collect();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = home.join(".cache/sprefa/go_checker").join(short);
    let binary = dir.join("go_checker");
    if binary.is_file() {
        return Ok(binary);
    }
    std::fs::create_dir_all(&dir).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("main.go"), DRIVER_MAIN).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("go.mod"), DRIVER_MOD).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("go.sum"), DRIVER_SUM).map_err(|err| stage(err.to_string()))?;
    let Some(out) = binary.to_str() else {
        return Err(stage("the cache path is not utf-8".to_string()));
    };
    match run_capped(&["go", "build", "-o", out, "."], &dir, &dir) {
        Capped::Exited { success: true, .. } => Ok(binary),
        Capped::Exited { stderr_tail, .. } => Err(stage(format!("go build: {stderr_tail}"))),
        Capped::Killed { secs } => Err(GoCheckerError::Budget(secs)),
        Capped::NotLaunched => Err(stage("go is not on PATH".to_string())),
    }
}

/// The wall cap, the process group and the file-backed stdout all come from
/// `run_capped`: the same discipline every scip indexer spawn runs under.
#[cfg(feature = "go-checker")]
pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    tsi: bool,
) -> Result<GoCheckerAnswers, GoCheckerError> {
    use crate::read::scip_ensure::{run_capped, Capped};

    let binary = staged_binary()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or_default();
    let dir =
        std::env::temp_dir().join(format!("sprefa-go-checker-{}-{nanos}", std::process::id()));
    let stage = GoCheckerError::NoDriver;
    std::fs::create_dir_all(&dir).map_err(|err| stage(err.to_string()))?;
    let request = dir.join("request.json");
    let body = serde_json::to_vec(&super::ts_checker::DriverRequest { root, files, tsi })
        .map_err(|err| stage(err.to_string()))?;
    std::fs::write(&request, body).map_err(|err| stage(err.to_string()))?;

    let (Some(binary), Some(request)) = (binary.to_str(), request.to_str()) else {
        return Err(stage("the temp path is not utf-8".to_string()));
    };
    match run_capped(&[binary, request], root, &dir) {
        Capped::Exited { success: true, .. } => {}
        Capped::Exited { stderr_tail, .. } => return Err(GoCheckerError::Failed(stderr_tail)),
        Capped::Killed { secs } => return Err(GoCheckerError::Budget(secs)),
        Capped::NotLaunched => return Err(stage("the staged sidecar did not launch".to_string())),
    }

    let stdout = std::fs::read_to_string(dir.join("indexer.stdout.log"))
        .map_err(|err| GoCheckerError::Failed(err.to_string()))?;
    let answers =
        super::ts_checker::parse_driver_stdout(&stdout, into_fact, GoCheckerError::Failed)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(answers)
}
