---
created: 2026-09-23
updated: 2026-09-26
type: task
status: open
priority: high
epic: scm-language-frontends
labels: [extract]
blocked_by: ['@scm-oxc-front-end']
lane: scm-frontends
lane_seq: 2
---

# SCM++ Kotlin per-file cutover

## Description
Move Kotlin single-file tree-sitter and SCM parsing, family projections, and module-reference syntax behind an SCM++ Kotlin feature. Reuse the language-row and module-syntax contracts established by Rust and exercised by TS/JS. Ryi continues to own corpus-level receiver and module resolution, TSI inference, and move planning. Preserve the existing K2 and K3 correctness tasks as separate work.

## Acceptance Criteria
- [ ] SCM++ Kotlin feature owns the per-file parse and returns owned requested family, fast-SCIP, and module-reference rows
- [ ] Kotlin and KTS module references and replacement spellings are produced by the language implementation
- [ ] Ryi performs no redundant per-file Kotlin parse or per-reference parser call for covered requests
- [ ] Kotlin family goldens, module-plane fixtures, receiver decline behavior, and move dry-runs match the pre-cutover baseline
- [ ] Combined-family parse/query counts are recorded

## Tests Run

## Implementation Notes

The existing `KotlinSource` owns the tree-sitter parse, SCM projections, and module facts. Current repro found two parse spans per `.kt` when requesting `cst,type,call,df`: one for the CST walk and one for the family projections. The family-owned tree is now reused for CST. Kotlin move planning in `sprefa-extract/src/edit/kotlin_rehome.rs` still scans files with `kt_parse`; replacement planning remains there pending a language module-syntax contract.

## Repro receipt

2026-09-26: before the fix, `HAFLEY_TRACE=/tmp/scm-kotlin-trace.json` on the two `kotlin_receivers` files recorded 4 Kotlin tree-sitter parse spans (2 per file) for combined CST/type/call/dataflow. After the fix, `combined_kotlin_families_share_one_tree_sitter_parse_per_file` asserts one span per file. Targeted nextest passed: `t_158_fast_scm_kotlin` (5 tests), `t_150_fast_scm_kotlin` (1), `t_4_move_kotlin` (5), `t_127_kotlin_modules` (3), `t_131_kotlin_module_resolve` (5), `t_137_kotlin_receiver_legs` (3), and `t_138_untyped_receiver_kotlin` (3). Remaining acceptance: Kotlin/KTS replacement spellings from the language module, parser-free move planning for covered references, and a combined-family query-count receipt.
