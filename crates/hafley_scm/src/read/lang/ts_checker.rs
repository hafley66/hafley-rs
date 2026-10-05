//! The ts CHECKER tier: the TypeScript compiler answers the DESTINATION of a
//! reference this crate's parse found; caller, site spans and drops stay ours.
//!
//! A per-lang copy of `rust_checker`, the way the resolve arms are; the
//! post-4d dedup sweep owns unifying the two.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{answer_of, CALL_FACETS, TYPE_FACETS};
pub use super::{
    CheckerAnswer as TsCheckerAnswer, CheckerAnswers as TsCheckerAnswers,
    CheckerRef as TsCheckerRef,
};
use crate::read::tsi::stamp_digests;
use crate::read::types::{ContentId, DefIndex};
use hafley_scm::span::Span;

/// Byte spans supplied by the parse. Only these positions are queried.
#[derive(Clone, Debug)]
pub struct TsSite {
    pub start: u32,
    pub end: u32,
    pub name: String,
    pub call: bool,
    pub type_ref: bool,
}

#[derive(Default)]
pub struct TsDemand {
    pub sites: Vec<TsSite>,
    /// Directed assignability questions, indexed into `sites`.
    pub pairs: Vec<(usize, usize)>,
}

type Bound = super::CheckerBound;

/// Why the tier could not run. Every one falls back to the syntax leg.
#[derive(Debug)]
pub enum TsCheckerError {
    NotBuilt,
    /// `node` is not on PATH, or the driver could not be staged.
    NoDriver(String),
    /// The driver ran and failed; the string is its last stderr line.
    Failed(String),
    Budget(u64),
}

impl std::fmt::Display for TsCheckerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBuilt => write!(
                f,
                "the ts checker tier needs --features ts-checker; falling back to the syntax leg"
            ),
            Self::NoDriver(detail) => write!(f, "no node driver: {detail}"),
            Self::Failed(detail) => write!(f, "the driver failed: {detail}"),
            Self::Budget(secs) => write!(f, "the driver exceeded {secs}s"),
        }
    }
}

/// Every answer joined ONCE to a `(blob, def span)` at build time; per-file
/// lists sorted by start, so a site lookup is a range scan, not a corpus walk.
#[derive(Default)]
pub struct TsCheckerIndex {
    pub version: String,
    calls: HashMap<String, Vec<Bound>>,
    /// A TypeF candidate carries no reference span, so the type plane keys on
    /// (file, name AS WRITTEN); a name one file resolves two ways binds nothing.
    types: HashMap<String, HashMap<String, Option<TsCheckerAnswer>>>,
    /// The walk's rows, span digests already substituted for the supplied paths
    /// the driver wrote.
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

impl TsCheckerIndex {
    /// An answer naming a file outside the resolve universe, or a def
    /// coordinate the parse never minted, is dropped: the syntax leg answers.
    pub fn build(
        answers: TsCheckerAnswers,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> TsCheckerIndex {
        let blob_of: HashMap<&str, &ContentId> = corpus
            .iter()
            .map(|(path, blob)| (path.as_str(), blob))
            .collect();
        let mut index = TsCheckerIndex {
            version: answers.version,
            load: answers.load,
            walk: answers.walk,
            files_answered: answers.files_answered,
            tsi: stamp_digests(answers.tsi, corpus),
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
            ..TsCheckerIndex::default()
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
                        index.external += matches!(answer, TsCheckerAnswer::External(_)) as usize;
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
            let mut by_name: HashMap<String, Option<TsCheckerAnswer>> = HashMap::new();
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
                index.external += matches!(answer, TsCheckerAnswer::External(_)) as usize;
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

    /// A site span covers the whole callee expression (`a.b.c`, `new Foo(x)`,
    /// `<Foo/>`): the answer is the RIGHTMOST inside it carrying the name.
    pub fn call_at(&self, path: &str, site: Span, callee: &str) -> Option<TsCheckerAnswer> {
        let bounds = self.calls.get(path)?;
        let end = site.end();
        bounds
            .iter()
            .filter(|bound| bound.start >= site.start && bound.end <= end && bound.name == callee)
            .next_back()
            .map(|bound| bound.answer.clone())
    }

    /// `name` is the candidate's `to` AS WRITTEN, dotted where the source
    /// dotted it (`ts.Node`), which is the text the driver keys on too.
    pub fn type_at(&self, path: &str, name: &str) -> Option<TsCheckerAnswer> {
        self.types.get(path)?.get(name)?.clone()
    }

    pub fn semantic_rows(&self) -> &[crate::read::tsi::FactOut] {
        &self.tsi
    }

    pub fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        &self.coverage
    }
}

impl crate::read::tsi::SemanticRows for TsCheckerIndex {
    fn facts(&self) -> &[crate::read::tsi::FactOut] {
        self.semantic_rows()
    }

    fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        TsCheckerIndex::coverage(self)
    }
}

/// The declaration identifier's offset picks between several defs of one name
/// in one file; a lone def of the name binds without it.
/// Run the checker over `root` and answer every reference in `files`
/// (supplied path, absolute path).
#[cfg(not(feature = "ts-checker"))]
pub fn answer(
    _root: &Path,
    _files: &[(String, PathBuf, TsDemand)],
    _tsi: bool,
) -> Result<TsCheckerAnswers, TsCheckerError> {
    Err(TsCheckerError::NotBuilt)
}

#[cfg(feature = "ts-checker")]
pub fn answer(
    root: &Path,
    files: &[(String, PathBuf, TsDemand)],
    tsi: bool,
) -> Result<TsCheckerAnswers, TsCheckerError> {
    super::tsgo_rows::answer(root, files, tsi).map_err(TsCheckerError::Failed)
}

/// The driver, embedded rather than installed: a tier that needs a separate
/// `npm install` to answer is a tier that silently does not run.
#[cfg(feature = "ts-checker")]
const DRIVER: &str = include_str!("ts_checker.mjs");

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
#[derive(serde::Serialize)]
pub(super) struct DriverRequest<'a> {
    pub(super) root: &'a Path,
    pub(super) files: &'a [(String, PathBuf)],
    /// The checker walk is the tier's expensive half and answers no resolve
    /// site, so it runs only for a stream that carries the TSI envelope.
    pub(super) tsi: bool,
}

/// One `[start, end, name, dst_path, dst_name, dst_offset]` wire row.
#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
type WireRow = (u32, u32, String, String, String, u32);

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
#[derive(serde::Deserialize)]
struct WireFile {
    path: String,
    calls: Vec<WireRow>,
    types: Vec<WireRow>,
    /// `[relation, arg, ...]` per row; the ordinal is the wire's, minted here.
    #[serde(default)]
    tsi: Vec<Vec<serde_json::Value>>,
}

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
#[derive(serde::Deserialize)]
struct WireStats {
    stats: WireCosts,
    #[serde(default)]
    coverage: Vec<(String, bool, Option<String>)>,
}

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
#[derive(serde::Deserialize)]
struct WireCosts {
    #[serde(rename = "loadMs")]
    load_ms: u64,
    #[serde(rename = "walkMs")]
    walk_ms: u64,
    files: usize,
}

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WireLine {
    File(WireFile),
    Stats(WireStats),
}

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
fn into_refs(rows: Vec<WireRow>) -> Vec<TsCheckerRef> {
    rows.into_iter()
        .map(
            |(start, end, name, dst_path, dst_name, dst_offset)| TsCheckerRef {
                start,
                end,
                name,
                dst_path,
                dst_name,
                dst_offset,
            },
        )
        .collect()
}

#[cfg(any(feature = "go-checker", feature = "ts-checker"))]
pub(super) fn parse_driver_stdout<E>(
    stdout: &str,
    mut into_fact: impl FnMut(Vec<serde_json::Value>) -> Result<crate::read::tsi::FactOut, E>,
    failed: impl Fn(String) -> E,
) -> Result<TsCheckerAnswers, E> {
    let mut answers = TsCheckerAnswers::default();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<WireLine>(line) {
            Ok(WireLine::File(file)) => {
                for row in file.tsi {
                    answers.tsi.push(into_fact(row)?);
                }
                answers
                    .calls
                    .insert(file.path.clone(), into_refs(file.calls));
                answers.types.insert(file.path, into_refs(file.types));
            }
            Ok(WireLine::Stats(WireStats { stats, coverage })) => {
                answers.load = Duration::from_millis(stats.load_ms);
                answers.walk = Duration::from_millis(stats.walk_ms);
                answers.files_answered = stats.files;
                answers.coverage = coverage;
            }
            Err(err) => return Err(failed(err.to_string())),
        }
    }
    Ok(answers)
}

/// One driver row `[relation, arg, ...]` into a fact. A row the registry does
/// not know, or an argument it cannot decode, stops the tier.
#[cfg(feature = "ts-checker")]
fn into_fact(row: Vec<serde_json::Value>) -> Result<crate::read::tsi::FactOut, TsCheckerError> {
    let mut parts = row.into_iter();
    let relation = parts
        .next()
        .and_then(|head| head.as_str().map(str::to_string))
        .ok_or_else(|| TsCheckerError::Failed("a tsi row opens with its relation".to_string()))?;
    let args: Vec<crate::read::tsi::Arg> = parts
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|err| TsCheckerError::Failed(format!("{relation}: {err}")))?;
    crate::read::tsi::registry::check(&relation, &args)
        .map_err(|detail| TsCheckerError::Failed(format!("{relation}: {detail}")))?;
    Ok(crate::read::tsi::FactOut {
        fact: 0,
        relation,
        args,
    })
}

/// The wall cap, the process group and the file-backed stdout all come from
/// `run_capped`: the same discipline every scip indexer spawn runs under.
#[cfg(feature = "ts-checker")]
pub fn answer_node_legacy(
    root: &Path,
    files: &[(String, PathBuf)],
    tsi: bool,
) -> Result<TsCheckerAnswers, TsCheckerError> {
    use crate::read::scip_ensure::{run_capped, Capped};
    let _checker_span =
        crate::read::trace::tracked(tracing::info_span!("typescript.checker")).entered();

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or_default();
    let dir =
        std::env::temp_dir().join(format!("sprefa-ts-checker-{}-{nanos}", std::process::id()));
    let stage = |detail: String| TsCheckerError::NoDriver(detail);
    std::fs::create_dir_all(&dir).map_err(|err| stage(err.to_string()))?;
    let script = dir.join("ts_checker.mjs");
    let request = dir.join("request.json");
    std::fs::write(&script, DRIVER).map_err(|err| stage(err.to_string()))?;
    let body = serde_json::to_vec(&DriverRequest { root, files, tsi })
        .map_err(|err| stage(err.to_string()))?;
    std::fs::write(&request, body).map_err(|err| stage(err.to_string()))?;

    let (Some(script), Some(request)) = (script.to_str(), request.to_str()) else {
        return Err(stage("the temp path is not utf-8".to_string()));
    };
    match run_capped(&["node", script, request], root, &dir) {
        Capped::Exited { success: true, .. } => {}
        Capped::Exited { stderr_tail, .. } => return Err(TsCheckerError::Failed(stderr_tail)),
        Capped::Killed { secs } => return Err(TsCheckerError::Budget(secs)),
        Capped::NotLaunched => {
            return Err(stage("node is not on PATH".to_string()));
        }
    }

    let stdout = std::fs::read_to_string(dir.join("indexer.stdout.log"))
        .map_err(|err| TsCheckerError::Failed(err.to_string()))?;
    let answers = parse_driver_stdout(&stdout, into_fact, TsCheckerError::Failed)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(answers)
}
