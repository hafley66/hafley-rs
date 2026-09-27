---
created: 2026-09-23
updated: 2026-09-26
type: task
status: obsolete
priority: high
epic: scm-language-frontends
labels: [extract]
blocked_by: ['@scm-rust-front-end']
lane: scm-frontends
lane_seq: 1
---

# SCM++ TS and JS OXC per-file cutover

## Description
Move TS, TSX, JS, and JSX single-file OXC parser passes and language-specific syntax projections behind an SCM++ parser feature. Reuse the Rust slice language-row contract and implement its module-syntax capability. Ryi keeps project resolution, TSI inference, and move planning.

## Acceptance Criteria
- [x] `hafley_scm::TsSource` owns OXC parse and per-file language projection for TS, TSX, JS, and JSX
- [x] One owned per-file result supplies requested family, fast-SCIP, and module-reference rows without a redundant ryi parse
- [x] Module references and replacement spellings preserve TS/JS import styles and path rules
- [x] Existing TS/JS family goldens, module resolution fixtures, and move dry-runs match the pre-cutover baseline
- [x] Combined-family parse/query counts are recorded and no per-reference parse is introduced

## Tests Run

## Implementation Notes

The existing TS/JS family, OXC specifier rows, and module-fact path is in `crates/hafley_scm/src/read/lang/ts.rs`. The only tree-sitter parser remaining in `ts_rehome.rs` handles JSON manifests; TS/JS module references and replacement spellings route through the SCM++ OXC rows and resolver. The Rust front-end decision does not block the verified TS/JS behavior.

## Repro receipt

2026-09-26: reproduced TS families and move rewrites with `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_160_fast_scm_ts::/) | test(/^t_41_move_ts::/)'` (7 passed); combined-family trace records one OXC parse plus the distinct CST parser, and source search finds no TypeScript tree-sitter scan or per-reference parse in `ts_rehome.rs`.
