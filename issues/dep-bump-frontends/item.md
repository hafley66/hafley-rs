---
created: 2026-09-19
updated: 2026-09-19
type: chore
status: open
priority: normal
epic: extract-parity-move-rename
related: ['@kind-vocab-constraint', '@default-families-no-conditional']
labels: [extract]
---

# bump ast-grep, oxc, ra_ap and syn to current

## Description

## Description

Measured 2026-09-19 against the crates.io API. Every front-end dependency is
behind, and `ast-grep` is behind by seven minor releases in the exact file the
kind-vocabulary work touches.

| dep | pinned | latest | gap |
| --- | --- | --- | --- |
| `ast-grep-core` | 0.38 | 0.45.3 | 7 minors |
| `ast-grep-language` | 0.38 | 0.45.3 | 7 minors |
| `ast-grep-config` | 0.38 | 0.45.3 | 7 minors |
| `oxc_allocator` / `oxc_parser` / `oxc_ast` / `oxc_ast_visit` / `oxc_span` / `oxc_syntax` / `oxc_semantic` | 0.135 | 0.150.0 | 15 minors |
| `syn` | 2 | 3.0.6 | major |
| `ra_ap_*` | 0.0.349 | 0.0.352 | 3 patches |
| `oxc_resolver` | 11.24 | 11.24.3 | patch |
| `ignore` | 0.4 | 0.4.33 | current |

### Why ast-grep first

`ast-grep-language` ships the tree-sitter grammars. Seven minors of grammar
updates change the node-kind vocabulary the call-kind table in
`src/lang/0_call_kinds.rs` enumerates, and the table was collected by dumping
`ryi --family cst` against the CURRENTLY pinned grammars. Bumping after the
table is written means re-collecting it; bumping first means collecting once.

Same file, same week: @kind-vocab-constraint wants `Node::kind_id() -> u16`
comparison instead of the kind strings. That API is stable across the range
(`ast-grep-core-0.38.7/src/node.rs:146`), but whatever else moved in seven
minors lands in `astgrep.rs` too.

### Sequencing

This bump and @kind-vocab-constraint may run in the same lane, user-set
2026-09-19. Bump first, re-collect the table against the new grammars, then do
the id work against the bumped API.

`syn` 2 -> 3 is a major and is the one item here that can break
`src/lang/rust*.rs` broadly. Split it out if the bump lane stalls on it; the
ast-grep and oxc bumps do not depend on it.

### Risk to the frozen goldens

`crates/sprefa-extract/tests/` carries frozen goldens over parse output. A
grammar bump moves them legitimately. Every golden that moves needs its diff
read, not blanket-regenerated: a changed row is either a grammar improvement or
a regression, and the two look identical in a regeneration.

## Acceptance Criteria

- [ ] `ast-grep-core`, `ast-grep-language`, `ast-grep-config` at 0.45.x
- [ ] `oxc_*` at 0.150.x
- [ ] `ra_ap_*` at 0.0.352
- [ ] `syn` at 3.x, or split to its own issue with the reason recorded
- [ ] `src/lang/0_call_kinds.rs` re-collected against the bumped grammars, with the dump command in the commit
- [ ] every moved golden has its diff read and the reason recorded, no blanket regeneration
- [ ] `cargo test --features cli --no-fail-fast`, zero failures
- [ ] the three receipt counts from @default-families-no-conditional re-measured and recorded, since a grammar bump moves them
