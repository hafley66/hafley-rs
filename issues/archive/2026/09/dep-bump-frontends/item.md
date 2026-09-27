---
created: 2026-09-19
updated: 2026-09-27
type: chore
status: fixed
priority: normal
epic: extract-parity-move-rename
related: ['@kind-vocab-constraint', '@default-families-no-conditional']
labels: [extract]
---

# bump ast-grep, oxc, ra_ap and syn to current

## Description

Measured 2026-09-19 against the crates.io API. Every front-end dependency is
behind, and `ast-grep` is behind by seven minor releases in the exact file the
kind-vocabulary work touches.

| dep | pinned | latest | gap |
| --- | --- | --- | --- |
| `ast-grep-core` | absent | 0.45.3 | no active manifest entry |
| `ast-grep-language` | absent | 0.45.3 | no active manifest entry |
| `ast-grep-config` | absent | 0.45.3 | no active manifest entry |
| `oxc_allocator` / `oxc_parser` / `oxc_ast` / `oxc_ast_visit` / `oxc_span` / `oxc_syntax` / `oxc_semantic` | 0.135 | 0.151.0 | 16 minors |
| `syn` | 2 | 3.0.6 | major |
| `ra_ap_*` | 0.0.349 | 0.0.352 | 3 patches |
| `oxc_resolver` | 11.24 | 11.24.3 | patch |
| `ignore` | 0.4 | 0.4.33 | current |

### Current-state check (2026-09-27)

`rg` over active `crates/**/Cargo.toml` finds no ast-grep dependency. The
current call-kind table is tree-sitter data at
`crates/hafley_scm/src/read/lang/0_call_kinds.rs`; its grammar collection is
independent of these OXC and rust-analyzer parser versions. OXC crates currently
publish 0.151.0, `ra_ap_*` 0.0.352, `syn` 3.0.6, and `oxc_resolver` 11.24.3.
This card covers the active OXC, resolver, and rust-analyzer pins; ast-grep is
recorded absent and is not added as a dependency.

### Grammar data check

The active call-kind table is tree-sitter data. No tree-sitter grammar dependency
changed in this card, so the table and its golden were not regenerated.

### Sequencing

The OXC and rust-analyzer version changes are in this card. `syn` 2 -> 3 is
split into `@dep-bump-syn-3`.

`syn` 2 -> 3 is a major and is the one item here that can break
`src/lang/rust*.rs` broadly. Split it out if the bump lane stalls on it; the
ast-grep and oxc bumps do not depend on it.

### Risk to the frozen goldens

`crates/sprefa-extract/tests/` carries frozen goldens over parse output. A
grammar bump moves them legitimately. Every golden that moves needs its diff
read, not blanket-regenerated: a changed row is either a grammar improvement or
a regression, and the two look identical in a regeneration.

## Acceptance Criteria

- [x] ast-grep absent from active manifests; do not add it
- [x] `oxc_*` at 0.151.x
- [x] `ra_ap_*` at 0.0.352
- [x] keep `syn` 2 and split its major migration into a separate issue with the reason recorded
- [x] `src/lang/0_call_kinds.rs` remains tree-sitter grammar data; no ast-grep grammar update applies
- [x] no tree-sitter grammar or golden moved; OXC and rust-analyzer upgrades do not change the grammar table
- [x] workspace and full sprefa-extract suites pass
- [x] default-family corpus receipt checked: the referenced 587-line `src/main.ts` is absent from this checkout; only the 16-line `crates/sprefa-extract/tests/fixtures/ts_checker/src/main.ts` and 3-line `ts_cross` main fixtures exist, so the historical 3281/7571/10852 counts could not be remeasured against the same source

## Plan

Update active OXC, resolver, and rust-analyzer dependencies. Keep the `syn`
2-to-3 migration separable. Preserve tree-sitter call-kind data because no
grammar dependency changes here.

## Decisions

### 2026-09-27 · @codex

Decision: split `syn` 2-to-3 into `@dep-bump-syn-3`. `syn` is shared across
sprefa-extract, hafley_scm, boop, hafley-observe, and macro crates; the major
AST migration can change parser call sites beyond this front-end version lane.
The active OXC and rust-analyzer pins remain in this card.

### Completion receipt · 2026-09-27 · @codex

- OXC dependencies updated to 0.151.0, `oxc_resolver` to 11.24.3, and
  rust-analyzer crates to 0.0.352 in both manifests and lockfiles.
- Adapted OXC AST consumers in `hafley_scm` and `sprefa-extract` for the 0.151
  API; `syn` remains at 2 with the split recorded in `@dep-bump-syn-3`.
- Workspace gate: `cargo nextest run --workspace -j 2 --no-fail-fast
  --status-level fail -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'`
  passed: 1355 passed, 203 skipped.
- Full sprefa-extract gate: `cargo nextest run --features cli -j 2
  --no-fail-fast --test all` passed on rerun after one transient timing failure:
  1114 passed, 18 skipped. The isolated timing test passed twice.
