---
created: 2026-09-23
updated: 2026-09-23
type: epic
owner: hafley66
status: needs-decision
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

Current checkout already has the TS/JS and Kotlin `Source` implementations in `hafley_scm`; a Kotlin combined-family duplicate parse was removed in `606b8771`. The ordered Rust → TS/JS → Kotlin cutover is gated on the user's pending Rust one-parse decision.

Question: What one-parse policy should the Rust front-end establish before the remaining language cutovers proceed?

Plan: After the Rust one-parse policy is resolved, make `hafley_scm` return one owned Rust file result for the requested families and module references, then verify TS/JS OXC rows and Kotlin module spellings flow through the same per-file contract without adding parse calls to move resolution; run the existing Rust, TS/JS, and Kotlin family and move fixture targets, record combined-family parse/query counts, and unblock SCIP ingestion only when all three language cutovers have receipts.
