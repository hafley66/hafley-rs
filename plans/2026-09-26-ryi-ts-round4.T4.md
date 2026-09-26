# ryi TypeScript versus CodeQL, lane T4

Status: code only. No cargo, ryi, CodeQL, or SCIP generation ran. Every row and count expectation below is **unverified**.

Baseline is T3's *prior validated step*, not a measurement of the T3 review follow-up: type 17350 agree / 13488 ryi-only / 326 CodeQL-only; call 4633 agree / 23040 ryi-only / 234 CodeQL-only. The T3 report explicitly says its later local-peer changes were not rerun.

## 1. Cross-file type re-exports

Cause and key verdict: `tsv2/runtime/types.ts` imports `SqlStatement` from `sprefa-store/js/src/engine/types.ts` and re-exports that imported type. The TypeScript binding used at a reference is the imported alias; its declaration is the engine type alias. Ryi's module resolver follows the export to the engine declaration. The CodeQL query's `exportsAs(name, ...)` plus `name.getADeclaration()` reports the declaration of the barrel's imported alias. Both coordinates describe the same binding at different alias depths. The comparison key `(source file, owner, target file, target name)` treats them as different edges.

T3 counted 226 cross-file CodeQL-only type keys. Of those, 212 name the barrel and 165 name its `SqlStatement` alias. The 165 should map to `sprefa-store/js/src/engine/types.ts:SqlStatement` for semantic comparison. This is a comparison-key change, so `crates/hafley_scm/src/read/lang/ts.rs` and its module resolver remain unchanged for this item.

Proposed **separate commit** for `crates/sprefa-extract/scripts/ryi-vs-codeql.sh`: retain the raw keys in `disagreements.tsv`, add a canonical declaration key for TypeScript type imports and re-exports, then compare canonical keys. Resolve aliases from the query's binding identity or a declaration-coordinate export table, not path-text substitution. Expected raw count delta: 0. Expected canonical delta for the 165 `SqlStatement` rows: 165 fewer CodeQL-only and, only where the corresponding ryi key exists, 165 fewer ryi-only and 165 more agreements. The remaining 47 barrel targets require per-binding mapping. These deltas are unverified.

## 2. Same-file type parameters

Cause: `TypeRefCollector` excludes a declaration's own type parameters from TypeF candidates. A type parameter is lexically bound within an alias, interface, class, function, or method; it has no corpus TypeF declaration node. CodeQL's `LocalTypeAccess` resolves these references to the same-file type-parameter declaration. T3's 100 same-file CodeQL-only type keys include 28 named `T`.

Fix: `crates/hafley_scm/src/read/lang/ts_resolve.rs` collects references to type parameters while traversing the already parsed AST, with lexical parameter scopes and the enclosing named owner. This now includes variable-bound arrows and function expressions. Anonymous classes and functions enter their own parameter scopes while inheriting the nearest named owner. `crates/hafley_scm/src/read/project.rs` emits one same-file resolved type row per `(owner span, parameter name)`. Without this emission dedup, `identity<T>(value: T): T` would write two identical SQLite rows; the comparison script's `select distinct` and the ladder's `select distinct` would collapse them downstream. Existing TypeF candidates remain excluded. No pin or golden changed.

New ladder source: `crates/sprefa-extract/tests/fixtures/ts_ladder/src/_23_type_params.ts`. Hand-written expected rows in `crates/sprefa-extract/tests/181_ts_ladder.rs`:

| Source | Kind | Owner | Target | Expected tier | Status |
| --- | --- | --- | --- | --- | --- |
| `_23_type_params.ts` | `generic` | `Boxed` | `_23_type_params.ts:T` | `f-` | unverified |
| `_23_type_params.ts` | `generic` | `echo` | `_23_type_params.ts:T` | `f-` | unverified |
| `_23_type_params.ts` | `generic` | `identity` | `_23_type_params.ts:T` | `f-` | unverified |
| `_23_type_params.ts` | `generic` | `makeHolder` | `_23_type_params.ts:T` | `f-` | unverified |
| `_23_type_params.ts` | `generic` | `makeHolder` | `_23_type_params.ts:U` | `f-` | unverified |

An anonymous generic default export with no enclosing named item remains outside this row model: the comparison query requires `itemName(source)`. A text scan of the retained T3 CodeQL source archive found 0 `export default class<` and 0 `export default function<` forms in the staged TypeScript files. This is a source-text count, not an AST count, and no new extraction ran.

Expected corpus delta: at least the 28 `T` keys move from CodeQL-only to agreement if their enclosing owner coordinates match. Additional names such as `Value`, `Key`, `In`, and `Out` may move; their count needs a fresh comparison. Each new agreement adds one ryi key and removes one CodeQL-only key. No exact post-change total is asserted without a run.

## 3. Same-file calls and duplicate names

Cause: generated `bind_args` is a top-level function repeated across generated files. Corpus-wide name uniqueness therefore fails. Its calls and definition are in the same source file. The 112 `bind_args` residual count was recorded **before** the T3 local-peer follow-up, so it is not a measurement of the current tree.

Fix: `ts_call_name_match` in `crates/hafley_scm/src/read/lang/ts.rs` now resolves a current-blob CallF node before counting corpus blobs. If no local CallF node matches, the corpus fallback requires one definition blob. This makes the duplicate-name guard apply only to the cross-file fallback; an unrelated exported peer cannot seat a local call. The public `call_name_match` path used by the SCIP ratchet is unchanged.

New ladder source: `crates/sprefa-extract/tests/fixtures/ts_ladder/src/_24_bind_args.ts` plus a same-named peer in `_25_bind_args_peer.ts`. Hand-written expected row in `crates/sprefa-extract/tests/181_ts_ladder.rs`:

| Source | Kind | Owner | Target | Expected tier | Status |
| --- | --- | --- | --- | --- | --- |
| `_24_bind_args.ts` | `call` | `arrival_statement` | `_24_bind_args.ts:bind_args` | `f-` | unverified |

The existing `_21_local_peer.ts useLocalHelper -> helper` row and this new row represent the same guard cause with different function names. Expected delta relative to the pre-follow-up T3 call count: up to 112 CodeQL-only `bind_args` keys move to agreement, contingent on matching owner coordinates. The incremental T4 delta cannot be separated from T3's unmeasured follow-up without a run. The post-follow-up count remains unverified.

## Review state

The two added ladder source files and all six expected ladder rows are unverified. No existing fixture, pin, or golden was edited.
