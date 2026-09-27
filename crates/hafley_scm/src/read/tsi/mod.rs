//! The type-system-interchange envelope riding the `FlatFact` JSONL wire:
//! protocol, run, witness, coverage, diagnostic.

use std::collections::HashMap;

use crate::read::types::ContentId;

/// Replace checker-supplied corpus paths in TSI span arguments with content
/// digests; paths outside the corpus remain unchanged.
pub(crate) fn stamp_digests(
    rows: Vec<types::FactOut>,
    corpus: &[(String, ContentId)],
) -> Vec<types::FactOut> {
    let digest_of: HashMap<&str, String> = corpus
        .iter()
        .map(|(path, blob)| (path.as_str(), blob.to_string()))
        .collect();
    rows.into_iter()
        .map(|mut row| {
            for arg in &mut row.args {
                if let types::Arg::Span(key, _, _) = arg {
                    if let Some(digest) = digest_of.get(key.as_str()) {
                        *key = digest.clone();
                    }
                }
            }
            row
        })
        .collect()
}

#[cfg(feature = "read")]
pub mod ingest;
pub mod registry;
pub mod semantic;
pub mod sink;
pub mod types;

#[cfg(feature = "read")]
pub use ingest::{ingest, IngestError};
pub use registry::{relation, ArgKind, Relation, REGISTRY};
pub use semantic::{emit_semantic, CoverageClaim, SemanticRows};
pub use sink::TsiSink;
pub use types::{
    Arg, CoverageOut, DiagnosticOut, FactOut, Method, Mode, RunOut, WitnessOut, PROTOCOL_VERSION,
};
