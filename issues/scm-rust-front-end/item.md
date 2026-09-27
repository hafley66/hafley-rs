---
created: 2026-09-23
updated: 2026-09-23
type: task
status: open
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
- [ ] One SCM++ extraction call supplies Rust CST, call, type, flow, fast-SCIP, and module-reference rows per file; no re-read or second parse in ryi for the same requested work
- [ ] Any remaining syn pass executes inside the SCM++ Rust feature and is measured explicitly
- [ ] A module-syntax capability returns written reference spans and can respell a resolved move; ryi still decides cross-file targets and edits
- [ ] RustSource no longer directly invokes syn::parse_file or tree-sitter for covered fast families
- [ ] Rust family goldens, fast-SCIP rows, and Rust move dry-runs match the pre-cutover baseline
- [ ] SCM++ validates Rust query capture kinds against the loaded grammar; no ryi ast-grep kind guess is retained
- [ ] A test records per-file parser/query invocation counts on a combined family request

## Tests Run

## Implementation Notes
