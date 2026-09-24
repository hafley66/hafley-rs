---
created: 2026-09-24
updated: 2026-09-24
type: bug
status: open
priority: normal
---

# Graph uses omits Rust ResolveRequest references

## Description

## Reproduction

In feature/ryi-project-graph, run: ryi graph --json --uses ResolveRequest crates/sprefa-extract/src/project.rs crates/sprefa-extract/src/0_graph.rs crates/sprefa-extract/src/5_diff.rs

Observed: zero graph_edge rows. The selected Rust files contain ResolveRequest imports, parameter types, return types, and struct literals (rg -n ResolveRequest on those files). The same graph command with --callers resolve_project_inputs returns four rows, so input routing works.

## Expected

At least the written Rust type references should be available as TypeF/TSI type-use facts or have an explicit coverage/decline receipt. Diagnose whether extraction, type-candidate creation, or graph --uses resolution drops them. Keep fast syntax separate from checker certainty.
