---
created: 2026-09-23
updated: 2026-09-23
type: epic
owner: hafley66
status: open
priority: high
labels: [extract]
---

# SCM++ language front ends for ryi fast

## Description
SCM++ owns every per-file parser pass and language-specific syntax projection. Parser backends are crate features: Rust tree-sitter plus optional syn, TS/JS OXC, and Kotlin tree-sitter. SCM++ returns owned per-file rows. Ryi owns corpus traversal, shared family storage, cross-file resolution, TSI inference, move planning, and soopy application.

Module syntax is a language capability in SCM++: extract module references and produce replacement spellings. Ryi resolves targets against the corpus and decides edits from the move map. Keep the existing ryi Source/Rehome contracts until a specific slice proves a narrower change necessary.

Execution order: Rust, TS/JS, Kotlin; then resume @scip-ingestion-conformance. Scope graph and compiler-backed SCIP ingestion are separate work.

## Acceptance Criteria
- [ ] Rust fast extraction consumes SCM++ per-file rows without a second ryi parse
- [ ] TS/JS OXC per-file parsing is owned by SCM++
- [ ] Kotlin per-file parsing and module syntax are owned by SCM++
- [ ] Rust, TS/JS, and Kotlin family goldens and move dry-runs retain their behavior
- [ ] @scip-ingestion-conformance is unblocked after the three language cutovers

## Tests Run

## Implementation Notes
