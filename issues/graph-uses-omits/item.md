---
created: 2026-09-24
updated: 2026-09-24
type: bug
status: needs-decision
priority: normal
commits:
- hash: 4b04e07b8c3f4ecbfbeff8f55828771e77a146ff
  summary: 'ryi: query witnessed paths across Git revisions'
- hash: a2d6317ccd5a3d69f3b1ebee2a66a0525c591612
  summary: 'ryi: resolve Rust callable signature type uses'
---

# Graph uses omits Rust ResolveRequest references

## Description

## Reproduction

In feature/ryi-project-graph, run: ryi graph --json --uses ResolveRequest crates/sprefa-extract/src/project.rs crates/sprefa-extract/src/0_graph.rs crates/sprefa-extract/src/5_diff.rs

Observed: zero graph_edge rows. The selected Rust files contain ResolveRequest imports, parameter types, return types, and struct literals (rg -n ResolveRequest on those files). The same graph command with --callers resolve_project_inputs returns four rows, so input routing works.

## Expected

At least the written Rust type references should be available as TypeF/TSI type-use facts or have an explicit coverage/decline receipt. Diagnose whether extraction, type-candidate creation, or graph --uses resolution drops them. Keep fast syntax separate from checker certainty.

## Comments

### 2026-09-27 · @codex

Current repro on `ryii`: `graph --uses ResolveRequest --root . crates/hafley_scm/src/read/project.rs crates/sprefa-extract/src/0_graph.rs crates/sprefa-extract/src/5_diff.rs` exits 0 with 15 rows, all from `crates/hafley_scm/src/read/project.rs`. The selected `sprefa-extract` files contain `ResolveRequest` imports, callable signatures and a struct literal, but contribute no rows. `--rust-checker` returns the same 15 rows.

### 2026-09-26T20:45:00Z · @codex

The Rust syntax collector now records struct-literal type paths as `uses`; `t_169_graph_uses_rust::rust_function_and_method_signatures_are_type_uses` passes. On this branch the original `crates/sprefa-extract/src/project.rs` path has moved to `crates/hafley_scm/src/read/project.rs`; querying that file with the three current files returns 15 rows, all from `project.rs`. The `sprefa-extract` files import `ResolveRequest` from another crate and produce no cross-crate edges. Decision needed on syntax-only cross-crate import resolution.

### 2026-09-24T06:28:12Z · @codex

a2d6317c adds Rust function, impl-method, and default-trait-method param/return type candidates in hafley_scm. Dogfood: ryi graph --uses ResolveRequest over project.rs, 0_graph.rs, and 5_diff.rs returns 14 rows. Expression-level struct literals and import references remain outside this candidate set; issue remains open for those sites or an explicit coverage receipt.

Decision needed: Should syntax-only graph resolution follow cross-crate imports and re-exports, or emit an explicit decline/coverage row for these references?
