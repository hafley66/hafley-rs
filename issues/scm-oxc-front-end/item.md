---
created: 2026-09-23
updated: 2026-09-26
type: task
status: needs-decision
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
- [ ] One owned per-file result supplies requested family, fast-SCIP, and module-reference rows without a redundant ryi parse
- [ ] Module references and replacement spellings preserve TS/JS import styles and path rules
- [ ] Existing TS/JS family goldens, module resolution fixtures, and move dry-runs match the pre-cutover baseline
- [ ] Combined-family parse/query counts are recorded and no per-reference parse is introduced

## Tests Run

## Implementation Notes

The existing TS/JS family and module-fact path is in `crates/hafley_scm/src/read/lang/ts.rs`. `sprefa-extract/src/edit/ts_rehome.rs` still contains a tree-sitter module scan, so replacement ownership and the no-redundant-parse acceptance remain open. This card is blocked by `scm-rust-front-end`; that issue is deferred by the user.

## Repro receipt

2026-09-26: `HAFLEY_TRACE=/tmp/scm-oxc-trace.json` on `tests/fixtures/ts_module/consumer.ts` with `--kinds cst,type,call,df` records one `tree-sitter` parse for CST and one `oxc` parse for the requested family/module facts. Source inspection shows `OxcParser` and `ts_stash_module_facts` in `hafley_scm/src/read/lang/ts.rs`; the separate `ts_rehome.rs` scan still constructs its own tree-sitter parser. Targeted nextest passed `t_160_fast_scm_ts` (3), `t_113_ts_module_edges` (3), and `t_41_move_ts` (4).

Question: Proceed with the remaining TS/JS module-syntax cutover independently while the Rust language-row contract is deferred?
