# SCM scopegraph lab

## Implemented gates

| Gate | Receipt |
|---|---|
| L1 relation predicates | Tests cover `#inside?`, `#has?`, `#precedes?`, and `#follows?` through `Query::general_predicates()` on Rust syntax. |
| Match-limit rail | Every `run_query` drains `matches()` and checks `did_exceed_match_limit()`; overflow returns `MatchLimit { path }`. |
| Grouped captures | The query runner uses `matches()` only; a two-capture fixture asserts both captures remain grouped in one result. |
| L2 Kotlin query | Helix Kotlin `locals.scm` is vendored with MPL-2.0/source header and 25 lab query lines; it compiles against `tree-sitter-kotlin-sg`, with call/import/alias/receiver/type captures exercised. |
| L3 local resolver | Scope nesting, innermost definition attachment, outward name lookup, and explicit unresolved reasons are implemented in 192 lines. |
| Kotlin corpus resolution | Explicit imports and aliases, wildcard imports, and same-package names resolve against exported top-level declarations. Import and package header tokens are excluded from local-name resolution. |
| Qualified-name edges | Each import path carries `Push`/`Pop` transitions between explicit stack states, with the source path retained on each edge. |
| Typed Kotlin receivers | Parameter types, constructor property types, and generic upper bounds resolve member accesses, including chained class properties, to the declaring file and definition span. |
| L5 TypeScript query | Helix `_typescript`, `ecma`, and `typescript` locals query files are vendored; the existing Kotlin-tested engine resolves a TypeScript parameter with no language-specific engine change. |

The query evaluator and local scope resolver total 456 lines. Kotlin `locals.scm` totals 73 lines, including source/license attribution and lab query additions.

Kotlin fixture comparison and the final verdict remain open gates. The comparison runs below exercise the receiver and module-resolution implementation.

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
