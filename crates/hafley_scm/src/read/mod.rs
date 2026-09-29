//! The read side sprefa-extract ryi drives: corpus -> normalized graph facts.
//! sprefa-extract re-exports this module at its crate root.
#![allow(dead_code)]

/// sprefa-extract's package version, the `ryi` every fact and trace names.
/// Pinned to sprefa-extract/Cargo.toml by tests/111_cli_identity.rs.
pub const RYI_VERSION: &str = "0.1.0";

#[path = "0_request_root.rs"]
mod request_root;
pub use request_root::{
    diagnostic_line, io_path, request_io_root, with_diagnostic_sink, with_io_root,
};

pub mod cache;
#[cfg(feature = "rust")]
pub mod cargo_metadata;
#[cfg(feature = "rust")]
pub use cargo_metadata::{
    load as cargo_workspace_metadata, targets as rust_cargo_targets, RustTarget,
};
pub mod cfg;
pub mod cpg_decode;
pub mod cpg_types;
#[cfg(feature = "read")]
pub mod deps;
pub mod dispatch;
pub mod family;
pub mod lang;
#[cfg(feature = "read")]
pub mod manifests;
#[cfg(feature = "read")]
pub mod project;
#[path = "1_reach.rs"]
#[cfg(feature = "read")]
pub mod reach;
pub mod rows;
pub mod schema;
pub mod scip;
pub mod scip_decode;
pub mod scip_ensure;
pub mod scip_rows;
pub mod scip_v5_rels;
pub mod seams;
pub mod shape;
#[path = "2_slow.rs"]
#[cfg(feature = "read")]
pub mod slow;
pub mod source;
pub mod trace;
pub mod tsi;
pub mod types;
pub mod wire;

pub use cfg::{
    build_cfg, cfg_bundle, cfg_facts, roles_for, CfgRole, RoleRule, GO_ROLES, KOTLIN_ROLES,
    RUST_ROLES, TS_ROLES,
};
pub use cpg_decode::decode_cpg_struct;
pub use cpg_types::{
    CpgEdge, CpgEdgeKind, CpgImport, CpgImportError, CpgNode, CpgNodeKind, CpgProperty,
    CpgPropertyValue,
};
#[cfg(feature = "read")]
pub use deps::{resolve_specifier, Policy, TsconfigPaths};
pub use dispatch::dispatch;
pub use family::{
    flow_edges, CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfArg, DfEdgeKind, DfF,
    DfFAux, DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, Family, FlowEdge, FlowEdgeKind,
    FlowF, MethodOwner, ProjectEdge, ResolutionOrigin, SigSlot, Specifier, SpecifierKind,
    TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeFAux, TypeSig,
};
#[cfg(feature = "commonlisp")]
pub use lang::CommonlispSource;
#[cfg(feature = "data")]
pub use lang::DataSource;
#[cfg(feature = "fallback")]
pub use lang::FallbackSource;
#[cfg(feature = "gdscript")]
pub use lang::GdscriptSource;
#[cfg(feature = "go")]
pub use lang::GoSource;
#[cfg(feature = "kotlin")]
pub use lang::KotlinSource;
#[cfg(feature = "markdown")]
pub use lang::MarkdownSource;
#[cfg(feature = "prolog")]
pub use lang::PrologSource;
#[cfg(feature = "python")]
pub use lang::PythonSource;
#[cfg(feature = "rust")]
pub use lang::RustSource;
pub use lang::{
    dl6_db_path, find_owned_region, open_dl6_readonly, open_readonly, owned_region_markers,
    propose_owned_region, query_source, query_source_facts, query_tree_sitter,
    query_tree_sitter_spans, scm_edges, scm_facts, source_for, sources, ByteRange, FactError,
    FactSet, GitBlobFact, OwnedRegion, OwnedRegionError, OwnedRegionProposal, RyiLang, ScmEdge,
    SourceCaptureFact, SourceMatchFact, SourcePlace, SourceQuery, SourceQueryError,
    SourceQueryFact, SourceQueryFacts, SourceQueryOutput, SourceReplacementFact,
    SourceRevisionFact, TreeSitterQuery, TreeSitterQueryMatch, TreeSitterSpannedCapture,
    TreeSitterSpannedMatch, DL6_DB_RELATIVE_PATH, SOURCE_FACT_PROTOCOL,
};
#[cfg(feature = "typescript")]
pub use lang::{respell, ts_specifiers, TsResolver, TsSource, TsSpecifier};
#[cfg(feature = "read")]
pub use manifests::{
    fold_package_edges, package_edges, package_edges_jsonl, Manifest, ManifestKind,
};
#[cfg(feature = "read")]
pub use project::{
    diet_scip, diet_scip_jsonl, diet_scip_streamed, diet_scip_with_raw, extract_pool,
    resolve_project, resolve_project_jsonl, resolve_project_target_with_raw,
    resolve_project_with_raw, resolve_project_with_raw_tsi,
    resolve_project_with_tsi_tiers, scip_facts, scip_facts_jsonl, scip_family,
    scip_family_from_index, scip_family_from_index_jsonl, scip_family_jsonl, scip_file_edges_jsonl,
    scip_index_location, sorted_lines, CheckerTier, DietRow, FsBlobSource, ProjectError,
    RawProjectFact, ResolveArm, ResolveArms, ResolveRequest, ResolveWithRawError,
    ScipFamilyRequest, ScipMode, SourceTreeBlobSource, CHECKER_TIERS, RESOLVE_ARMS,
};
#[cfg(feature = "read")]
pub use reach::{reach_files, REACH_DEPTH_CAP};
pub use rows::{Edge, FamilyBundle, Node};
pub use scip::{
    byte_range, byte_range_cached, copy_sources, definition_of, join_documents, site_occurrence,
    Fallback, IndexerSpec, ScipClang, ScipGo, ScipJava, ScipPython, ScipRust, ScipTypescript,
    Staging,
};
pub use scip_ensure::{
    default_cache_dir, detect, detect_picked, ensure_index, ensure_index_for_set,
    ensure_index_picked, external_cache_dir, fresh_index_for_set, index_path, index_path_for_set,
    indexer_langs, pick_cache_dir, record_index_set, root_key, source_set_for_root, EnsureReport,
    IndexBudget, IndexSet, Indexer, IndexerPick, IndexerSkip, SkipReason, INDEXERS,
};
pub use scip_rows::{flatten_scip_records, ScipRecords, SCIP_RECORD_KINDS};
pub use scip_v5_rels::v5_rel_rows;
pub use seams::{
    build_def_index, containing_def_site, containing_def_site_in, corpus_defs, covering_def,
    def_named, own_blob, BlobSource, DefIndex, DefSite, FileSet, IndexBag, ManifestMap,
    OccurrenceRole, ParseError, Parser, PositionEncoding, Project, ProjectCx, ProjectDigest,
    Resolve, ScipDiagnostic, ScipDocument, ScipError, ScipIndex, ScipMetadata, ScipOccurrence,
    ScipRelationship, ScipSignature, ScipSource, ScipSymbolInfo,
};
pub use shape::{
    content_id_of, ContentId, FamilyTag, NameId, NodeRef, Span, Strings, ZERO_CONTENT_ID,
};
#[cfg(feature = "read")]
pub use slow::{slow_project, slow_project_with_raw};
pub use soopy::{
    ContentId as SourceContentId, Pattern as SourcePattern, ReadRequest as SourceReadRequest,
    RepositoryId as SourceRepositoryId, Revision as SourceRevision, RevisionId as SourceRevisionId,
    SourceEntry, SourceRef,
};
pub use source::{FamilyMask, RyiOutput, Source};
pub use types::{CfgEdgeKind, CfgF, CfgNodeKind, SymbolId, SymbolInterner};
pub use wire::{
    file_fact, file_fact_with_content_id, flatten, flatten_cfg, flatten_cfg_each, flatten_each,
    flatten_flow, flatten_jsonl, flatten_scip, line_start_fact_with_content_id, newline_offsets,
    scip_file_edges, size_skip_fact, FlatFact, SpanOut, DEFAULT_MAX_BYTES, SCHEMA,
};
