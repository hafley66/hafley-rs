//! The rust CHECKER tier: rust-analyzer answers the DESTINATION of a reference
//! this crate's parse found; caller, site spans and drops stay ours.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::{field_reads, FieldProbe, FieldRead};

#[cfg(feature = "rust-checker")]
pub use super::rust_checker_session::warm_workspace_available;

#[cfg(not(feature = "rust-checker"))]
pub fn warm_workspace_available(_root: &std::path::Path, _tier: LoadMode) -> bool {
    false
}
pub use super::CheckerAnswer;
use super::{answer_of, CALL_FACETS};
use crate::read::shape::FamilyTag;
use crate::read::tsi::stamp_digests;
use crate::read::types::{ContentId, DefIndex};
use hafley_scm::span::Span;

/// One resolved reference. Offsets are UTF-8 byte offsets into the file.
#[derive(Clone, Debug)]
pub struct CheckerRef {
    pub start: u32,
    pub end: u32,
    pub name: String,
    /// The path as WRITTEN, segments joined by `::` and no generic arguments:
    /// the spelling a `TypeEdgeCandidate` carries. Empty on the call plane.
    pub written: String,
    /// Empty when the checker resolved the reference OUTSIDE the resolve
    /// universe: std, a dependency, a file this run was not handed.
    pub dst_path: String,
    pub dst_name: String,
    /// The declaration identifier's offset: several defs in one file share a name.
    pub dst_offset: u32,
}

/// The loader's return: resolved references per referring file, plus the two
/// costs the tier is judged on separately.
#[derive(Default)]
pub struct CheckerAnswers {
    pub calls: HashMap<String, Vec<CheckerRef>>,
    pub types: HashMap<String, Vec<CheckerRef>>,
    /// The item walk's own rows, ids run-local across the whole workspace. Empty
    /// unless the caller asked for them: the walk is not free.
    pub tsi: Vec<crate::read::tsi::FactOut>,
    /// A claim about the whole run, never a file.
    pub coverage: Vec<crate::read::tsi::CoverageClaim>,
    /// `cargo metadata` plus the salsa workspace load.
    pub load: Duration,
    /// The per-file resolve walk over the loaded workspace.
    pub walk: Duration,
    pub files_answered: usize,
    /// Supplied paths the loaded crate graph declares no module for: `cfg`-gated
    /// out, or outside every crate root. Filled only by the item walk.
    pub unmodulated: Vec<String>,
    /// Every `MethodCallExpr` the walk visited, and the count rust-analyzer
    /// declined to name a function for: the tier's own answer-coverage gap.
    pub method_sites: usize,
    pub method_unresolved: usize,
}

/// Names loads workspace def maps without a sysroot or inference.
/// Types loads dependencies and the sysroot for inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoadMode {
    Names,
    Types,
}

/// Why a rust-analyzer host could not answer.
#[derive(Debug)]
pub enum CheckerError {
    NotBuilt,
    NeedsTypes,
    NoWorkspace(String),
    Budget(Duration),
    /// A demand walk popped a body past its deadline.
    Deadline,
}

impl std::fmt::Display for CheckerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsTypes => write!(f, "needs_types"),
            Self::NotBuilt => write!(
                f,
                "rust-analyzer Names and Types need --features rust-checker"
            ),
            Self::NoWorkspace(detail) => write!(f, "no cargo workspace: {detail}"),
            Self::Budget(budget) => {
                write!(f, "workspace load exceeded {:.0}s", budget.as_secs_f64())
            }
            Self::Deadline => write!(f, "the demand walk passed its deadline"),
        }
    }
}

/// One resolved reference, already joined to a corpus definition coordinate.
#[derive(Clone, Debug)]
struct Bound {
    start: u32,
    end: u32,
    name: String,
    written: String,
    answer: CheckerAnswer,
}

/// Every answer joined ONCE to a `(blob, def span)` at build time; per-file
/// lists sorted by start, so a site lookup is a range scan, not a corpus walk.
#[derive(Default)]
pub struct RustCheckerIndex {
    calls: HashMap<String, Vec<Bound>>,
    /// A TypeF candidate carries an OWNER span and no reference span, so a
    /// name one file resolves two ways is answered by the reference nearest
    /// the owner. Sorted by start, as `calls` is.
    types: HashMap<String, Vec<Bound>>,
    /// The walk's rows, span digests already substituted for the supplied paths
    /// the walk wrote.
    tsi: Vec<crate::read::tsi::FactOut>,
    coverage: Vec<crate::read::tsi::CoverageClaim>,
    /// Answers naming a corpus file whose parse minted no def there; they fall
    /// back to the syntax leg, so this is the tier's own miss count.
    pub unjoined: usize,
    /// References the checker resolved outside the corpus.
    pub external: usize,
    /// Type names one file resolved two ways: the owner-distance pick stands
    /// in for the reference span the candidate does not carry.
    pub type_ambiguous: usize,
    pub load: Duration,
    pub walk: Duration,
    pub files_answered: usize,
    /// Supplied paths the walk found owning no module in the loaded graph.
    pub unmodulated: Vec<String>,
    pub method_sites: usize,
    pub method_unresolved: usize,
}

impl RustCheckerIndex {
    /// An answer naming a file outside the resolve universe, or a def
    /// coordinate the parse never minted, is dropped: the syntax leg answers.
    pub fn build(
        answers: CheckerAnswers,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> RustCheckerIndex {
        let blob_of: HashMap<&str, &ContentId> = corpus
            .iter()
            .map(|(path, blob)| (path.as_str(), blob))
            .collect();
        let mut index = RustCheckerIndex {
            load: answers.load,
            walk: answers.walk,
            files_answered: answers.files_answered,
            unmodulated: answers.unmodulated,
            method_sites: answers.method_sites,
            method_unresolved: answers.method_unresolved,
            tsi: stamp_digests(answers.tsi, corpus),
            coverage: answers.coverage,
            ..RustCheckerIndex::default()
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
                        index.external += matches!(answer, CheckerAnswer::External(_)) as usize;
                        bounds.push(Bound {
                            start: reference.start,
                            end: reference.end,
                            name: reference.name,
                            written: reference.written,
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
            let mut bounds: Vec<Bound> = Vec::with_capacity(refs.len());
            let mut first_answer: HashMap<String, (CheckerAnswer, bool)> = HashMap::new();
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
                index.external += matches!(answer, CheckerAnswer::External(_)) as usize;
                match first_answer.entry(reference.name.clone()) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert((answer.clone(), false));
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        let (first, counted) = slot.get_mut();
                        if *first != answer && !*counted {
                            index.type_ambiguous += 1;
                            *counted = true;
                        }
                    }
                }
                bounds.push(Bound {
                    start: reference.start,
                    end: reference.end,
                    name: reference.name,
                    written: reference.written,
                    answer,
                });
            }
            bounds.sort_by_key(|bound| (bound.start, bound.end));
            index.types.insert(path, bounds);
        }
        index
    }

    /// A site span covers the whole callee path (`a::b::c`) or a bare method
    /// ident, so the answer is the RIGHTMOST inside it carrying the callee name.
    pub fn call_at(&self, path: &str, site: Span, callee: &str) -> Option<CheckerAnswer> {
        let bounds = self.calls.get(path)?;
        let end = site.end();
        bounds
            .iter()
            .filter(|bound| bound.start >= site.start && bound.end <= end && bound.name == callee)
            .next_back()
            .map(|bound| bound.answer.clone())
    }

    /// The answer for a candidate spelled `written` (trailing name `name`) in
    /// `path`. Every reference of the name agreeing is the common case. Two
    /// answers narrow first to the references spelled exactly as the candidate
    /// (`decoys::Config` against `Config`), then to the one nearest `owner`: a
    /// candidate's references sit in its owner's header (fields, bounds, the
    /// impl line), so the nearest is the owner's own.
    pub fn type_at(
        &self,
        path: &str,
        name: &str,
        written: &str,
        owner: Span,
    ) -> Option<CheckerAnswer> {
        let bounds = self.types.get(path)?;
        let mut named = bounds.iter().filter(|bound| bound.name == name);
        let first = named.next()?;
        if named.all(|bound| bound.answer == first.answer) {
            return Some(first.answer.clone());
        }
        let spelled: Vec<&Bound> = bounds
            .iter()
            .filter(|bound| bound.name == name && bound.written == written)
            .collect();
        let pool: Vec<&Bound> = if spelled.is_empty() {
            bounds.iter().filter(|bound| bound.name == name).collect()
        } else {
            spelled
        };
        pool.into_iter()
            .min_by_key(|bound| bound.start.abs_diff(owner.start))
            .map(|bound| bound.answer.clone())
    }

    pub fn semantic_rows(&self) -> &[crate::read::tsi::FactOut] {
        &self.tsi
    }

    pub fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        &self.coverage
    }
}

impl crate::read::tsi::SemanticRows for RustCheckerIndex {
    fn facts(&self) -> &[crate::read::tsi::FactOut] {
        self.semantic_rows()
    }

    fn coverage(&self) -> &[crate::read::tsi::CoverageClaim] {
        RustCheckerIndex::coverage(self)
    }
}

const TYPE_FACETS: &[FamilyTag] = &[FamilyTag::Type];

/// Run the checker over `root` and answer every reference in `files`
/// (supplied path, absolute path).
#[cfg(not(feature = "rust-checker"))]
pub fn answer(
    _root: &Path,
    _files: &[(String, PathBuf)],
    _budget: Duration,
    _tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    Err(CheckerError::NotBuilt)
}

#[cfg(feature = "rust-checker")]
pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    budget: Duration,
    tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    super::rust_checker_ra::answer(root, files, budget, tsi)
}

#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::TargetTypeReference;
#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::{TargetCall, TargetCalls};

#[cfg(feature = "rust-checker")]
pub fn target_calls(
    root: &Path,
    files: &[(String, PathBuf)],
    seeds: &[(String, String)],
    tier: LoadMode,
    budget: Duration,
) -> Result<TargetCalls, CheckerError> {
    super::rust_checker_ra::target_calls(root, files, seeds, tier, budget)
}

#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::{
    resolve_written_method, all_module_places, module_places, module_tree, module_tree_for_workspace, module_tree_for_workspace_files, resolve_method,
    dependency_places, resolve_path, resolve_path_at, resolve_prefix, scope_names, scope_names_at, Abstain, DefPlace, ModulePlace,
    NamesHost, RustModuleTree,
};
#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::{
    demand_walk, BodyEdge, BodyEdges, EdgeKind, WalkAnswer, WalkEdge, WalkNode, WalkQuestion,
    WalkSession,
};
#[cfg(feature = "rust-checker")]
pub use super::rust_checker_ra::{rename, RenameEdit, RenameFailure, RenameSeed};
#[cfg(feature = "rust-checker")]
pub use super::rust_workspace::ManifestKey;

#[cfg(feature = "rust-checker")]
pub fn target_types(
    root: &Path,
    files: &[(String, PathBuf)],
    seeds: &[(String, String)],
    budget: Duration,
) -> Result<Vec<TargetTypeReference>, CheckerError> {
    super::rust_checker_ra::target_types(root, files, seeds, budget)
}
