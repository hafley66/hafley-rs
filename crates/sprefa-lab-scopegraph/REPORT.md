# SCM scopegraph lab

## Implemented gates

| Gate | Receipt |
|---|---|
| Kotlin declaration scope | Forward `later()` was red as `NoDefinition`; top-level and class-member properties now bind regardless of source order, while a later function-body `val item` remains `DefinitionAfterReference`. Local suite passes 14/14. |
| L1 relation predicates | Tests cover `#inside?`, `#has?`, `#precedes?`, and `#follows?` through `Query::general_predicates()` on Rust syntax. |
| Match-limit rail | Every `run_query` drains `matches()` and checks `did_exceed_match_limit()`; overflow returns `MatchLimit { path }`. |
| Grouped captures | The query runner uses `matches()` only; a two-capture fixture asserts both captures remain grouped in one result. |
| L2 Kotlin query | Helix Kotlin `locals.scm` is vendored with MPL-2.0/source header and 25 lab query lines; it compiles against `tree-sitter-kotlin-sg`, with call/import/alias/receiver/type captures exercised. |
| L3 local resolver | Scope nesting, innermost definition attachment, outward name lookup, and explicit unresolved reasons are implemented in 192 lines. |
| Kotlin corpus resolution | Explicit imports and aliases, wildcard imports, and same-package names resolve against exported top-level declarations. Import and package header tokens are excluded from local-name resolution. |
| Qualified-name edges | Each import path carries `Push`/`Pop` transitions between explicit stack states, with the source path retained on each edge. |
| Typed Kotlin receivers | Parameter types, constructor property types, and generic upper bounds resolve member accesses, including chained class properties, to the declaring file and definition span. |
| L5 TypeScript query | Helix `_typescript`, `ecma`, and `typescript` locals query files are vendored; the existing Kotlin-tested engine resolves a TypeScript parameter with no language-specific engine change. |

The query evaluator and local scope resolver total 486 lines. Kotlin `locals.scm` totals 73 lines, including source/license attribution and lab query additions.

The forward-declaration reproducer passed after the scope fix; the lab suite is 14/14. The checked-in Ryi Kotlin receiver/module fixture tests passed 8/8. The pinned SCIP judge and fast-SCM ratchet tests passed 3/3. The byte-equal ryi/ryii e2e script passed 14/14 from a lane-scratch copy configured to keep temporary output outside `/tmp`. The workspace suite passed 1379/1379 with 203 skipped and one previously recorded leaky case (`boop-acp::channel::claude::tests::streamed_activity_is_reported_to_the_stall_watchdog`).

The L4 lab-versus-`ryi fast` resolution/unresolved-set comparison and the L6 lab-edge SCIP comparison plus replacement verdict remain open gates. The commands below exercise the current Kotlin receiver/module and pinned SCIP implementations; the lab output comparison and 61% line-count verdict still need measurements.

## Remaining measurement gates

Run the Kotlin receiver and module-plane fixtures against current `ryi fast`, then compare the local unresolved set and list each difference:

```sh
cd crates/sprefa-extract
HAFLEY_TRACE=1 cargo nextest run --features cli -j 2 --test all -E 'test(/^t_137_kotlin_receiver_legs::/) | test(/^t_131_kotlin_module_resolve::/)'
```

Run the checked-in SCIP judge and fast scope-graph ratchet:

```sh
cd crates/sprefa-extract
HAFLEY_TRACE=1 cargo nextest run --features cli -j 2 --test all -E 'test(/^t_159_fast_scm_judge::/) | test(/^t_161_fast_scm_ratchet::/)'
```

The final replacement verdict and line-count comparison to the 61% per-language baseline remain pending those receipts.

Property order follow-up: the top-level and class-member property references were red with no definition for `fun f() = x; val x = 1`; both resolve after the change. The function-body local still reports `DefinitionAfterReference`.
