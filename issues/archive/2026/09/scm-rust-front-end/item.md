---
created: 2026-09-23
updated: 2026-09-27
type: task
status: fixed
priority: high
epic: scm-language-frontends
labels: [extract]
lane: scm-frontends
lane_seq: 0
---

# SCM++ Rust per-file cutover and module syntax

## Description
First vertical. SCM++ Rust feature owns the tree-sitter parse, RUST_SCM and call query execution, Rust-specific TypeF/CallF/DfF input projection, and any required syn fallback. Define the narrow language extraction and module-syntax capability using crate-owned per-file rows. Ryi consumes those rows and retains its family vocabulary, cross-file resolver, MoveCx, Rehome orchestration, and soopy plan.

## Acceptance Criteria
- [x] One SCM++ extraction call supplies Rust CST, call, type, flow, fast-SCIP, and module-reference rows per file; no re-read or second parse in ryi for the same requested work
- [x] Any remaining syn pass executes inside the SCM++ Rust feature and is measured explicitly
- [x] A module-syntax capability returns written reference spans and can respell a resolved move; ryi still decides cross-file targets and edits
- [x] RustSource no longer directly invokes syn::parse_file or tree-sitter for covered fast families
- [x] Rust family goldens, fast-SCIP rows, and Rust move dry-runs match the pre-cutover baseline
- [x] SCM++ validates Rust query capture kinds against the loaded grammar; no ryi ast-grep kind guess is retained
- [x] A test records per-file parser/query invocation counts on a combined family request

## Tests Run

- `t_31_tracing::phase_calls_per_file_are_pinned`: combined Rust CST/type/call/flow request records one parse and one combined query per file.
- `cargo nextest run --features cli -j 2 --offline --locked --test all`: 1117/1117 passed, 18 skipped.
- `bash scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/debug`: 14/14 passed.
- `cargo nextest run --workspace -j 2 --offline --locked -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'`: 1366 passed, 203 skipped, 1 leaky, 0 failed.

## Implementation Notes

- `RustFastFile::extract` in `hafley_scm::lang::rust` owns Rust tree-sitter parsing and the combined call/fast query. `RustSource` consumes its tree and arena; family projections and module rows remain crate-owned.
- Trace phase counts pin parser calls and combined-query calls per file. The Rust query is compiled against the loaded tree-sitter Rust grammar.
