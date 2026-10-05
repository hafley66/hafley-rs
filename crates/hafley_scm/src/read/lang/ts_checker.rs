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
    /// The native tsgo executable could not be launched.
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
            Self::NoDriver(detail) => write!(f, "no tsgo executable: {detail}"),
            Self::Failed(detail) => write!(f, "tsgo failed: {detail}"),
            Self::Budget(secs) => write!(f, "tsgo exceeded {secs}s"),
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
    /// the session supplied.
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
    /// dotted it (`ts.Node`), which is the requested position spelling.
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
