---
created: 2026-09-23
updated: 2026-09-27
type: epic
owner: hafley66
status: fixed
priority: high
labels: [extract]
---

# SCM++ language front ends for ryi fast

## Description
SCM++ owns every per-file parser pass and language-specific syntax projection. Parser backends are crate features: Rust tree-sitter plus optional syn, TS/JS OXC, and Kotlin tree-sitter. SCM++ returns owned per-file rows. Ryi owns corpus traversal, shared family storage, cross-file resolution, TSI inference, move planning, and soopy application.

Module syntax is a language capability in SCM++: extract module references and produce replacement spellings. Ryi resolves targets against the corpus and decides edits from the move map. Keep the existing ryi Source/Rehome contracts until a specific slice proves a narrower change necessary.

Execution order: Rust, TS/JS, Kotlin; then resume @scip-ingestion-conformance. Scope graph and compiler-backed SCIP ingestion are separate work.

## Acceptance Criteria
- [x] Rust fast extraction consumes SCM++ per-file rows without a second ryi parse
- [x] TS/JS OXC per-file parsing is owned by SCM++
- [x] Kotlin per-file parsing and module syntax are owned by SCM++
- [x] Rust, TS/JS, and Kotlin family goldens and move dry-runs retain their behavior
- [x] @scip-ingestion-conformance is unblocked after the three language cutovers

## Tests Run

- Rust `DL_TRACE_SUMMARY=1 ryii fast tests/fixtures/rust/sample.rs`: parse calls=1, query calls=1.
- TS `DL_TRACE_SUMMARY=1 ryii fast tests/fixtures/ts/sample.ts`: parse calls=2 (tree-sitter plus OXC); the OXC parse and projections are in `hafley_scm::read::lang::ts`.
- Kotlin `DL_TRACE_SUMMARY=1 ryii fast tests/fixtures/df_loops/sample.kt`: parse calls=1.
- `cargo nextest run --features cli -j 2 --offline --locked --test all`: 1118/1118 passed, 18 skipped; includes language goldens and Rust/TS/Kotlin move tests.
- `bash scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/debug`: 14/14 passed.
- `cargo nextest run --workspace -j 2 --offline --locked -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'`: 1366 passed, 203 skipped, 2 slow, 1 leaky, 0 failed.

## Implementation Notes

- `RustFastFile::extract` owns the Rust parse and combined query. Rust, TS/JS, and Kotlin `Source` implementations and module syntax remain in `hafley_scm`; ryi consumes the resulting per-file output and owns cross-file resolution and move planning.
- Combined-family receipts: Rust parse/query 1/1, TS tree-sitter/OXC parse calls 2, Kotlin parse 1. Existing family goldens and module respelling/move tests pass; SCIP ingestion is unblocked.
