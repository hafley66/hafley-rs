---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: normal
---

# ryi: reachability stops at trait-object calls (`dyn Trait`)

## Description
Effects propagated up `resolved_edge` from boop's 2293 effect sites never reach a CLI handler for http, ws, jsonrpc or task. The call chain passes through trait objects (`dyn Harness`, `Door`, `LiveSessions`, `Multiplexer`), and a call on `dyn Trait` resolves to the trait method declaration, not to its implementors. Reachability, effects and callers all stop there.

## Evidence
- `plans/boop-effects/3_fn_effect.tsv`: `run_cli` reaches channel clock env fs lock log net process signal sqlite thread tmux; no http, ws, jsonrpc, task.
- boop-harness has http 31, ws 15 sites; boop-acp jsonrpc 50 (`1_effect_crate.tsv`).
- 8 traits, 79 implementors (boop2 `6_topology.tsp` `TRAIT_IMPLS`).

## Wanted
A trait-method call site gains edges to every corpus implementor of that method (rust-analyzer: `Trait` impls via `hir::Impl::all_for_trait`), marked as dispatch edges so reachability can include or exclude them.
