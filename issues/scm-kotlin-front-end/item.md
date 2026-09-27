---
created: 2026-09-23
updated: 2026-09-26
type: task
status: fixed
priority: high
epic: scm-language-frontends
labels: [extract]
lane: scm-frontends
lane_seq: 2
---

# SCM++ Kotlin per-file cutover

## Description
Move Kotlin single-file tree-sitter and SCM parsing, family projections, and module-reference syntax behind an SCM++ Kotlin feature. Reuse the language-row and module-syntax contracts established by Rust and exercised by TS/JS. Ryi continues to own corpus-level receiver and module resolution, TSI inference, and move planning. Preserve the existing K2 and K3 correctness tasks as separate work.

## Acceptance Criteria
- [x] SCM++ Kotlin feature owns the per-file parse and returns owned requested family, fast-SCIP, and module-reference rows
- [x] Kotlin and KTS module references and replacement spellings are produced by the language implementation
- [x] Ryi performs no redundant per-file Kotlin parse or per-reference parser call for covered requests
- [x] Kotlin family goldens, module-plane fixtures, receiver decline behavior, and move dry-runs match the pre-cutover baseline
- [x] Combined-family parse/query counts are recorded

## Tests Run

- `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_158_fast_scm_kotlin::/) | test(/^t_4_move_kotlin::/)'`: 10 passed
- `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_127_kotlin_modules::/) | test(/^t_131_kotlin_module_resolve::/) | test(/^t_137_kotlin_receiver_legs::/) | test(/^t_138_untyped_receiver_kotlin::/)'`: 17 passed
- `cargo nextest run -p hafley_scm --features read -j 2 -E 'test(/move_syntax_owns_spans_and_package_replacement/)'`: 1 passed
- `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_3_move_rust::/) | test(/^t_41_move_ts::/) | test(/^t_4_move_kotlin::/)'`: 28 passed

## Implementation Notes

`KotlinSource` owns the combined-family tree and module facts. `kotlin_modules` also owns move facts, package transitions, and import replacement spellings. Ryi builds one owned syntax result per Kotlin file in the move corpus, reuses the moved file's facts from its plan, and emits a respell batch without invoking a parser per reference.

## Repro receipt

2026-09-26: `t_158_fast_scm_kotlin` asserts one tree-sitter parse per file for combined CST/type/call/df; module binding and receiver decline fixtures pass; Kotlin move dry-runs preserve package/import spellings. Move facts, spans, path transitions, and replacement rules are tested in `hafley_scm`. Implementation: `343d0915`.
