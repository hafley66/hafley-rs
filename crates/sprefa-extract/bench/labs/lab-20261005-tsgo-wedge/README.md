# lab-20261005-tsgo-wedge

Question: the least code to add to TypeScript 7 (tsgo) so ryi's slow TypeScript tier gets definitions,
symbol/type identity and type relations over one batched channel, retiring the TS5 Node script.
Brief: `plans/2026-10-05-tsgo-wedge-lab.brief.md`.

## Source and versions

| item | value |
| --- | --- |
| Go source home | `microsoft/TypeScript`, directory `tsc/` (microsoft/typescript-go is closed; its last commit 89d5d5b2 "Add closure notice and link to original repo" points there) |
| tag matching npm `typescript@7.0.2` | `v7.0.2` = `1e4744d68260a7cb91b62b12edc3f6a2187faaf1` ("Merge branch 'main' into ts7-release"); `tsc/internal/core/version.go:8` = `7.0.2` |
| npm package `gitHead` | `2bd066d87f5bafd315be9f40889d0a60b9e58e0b`, not fetchable from the remote ("not our ref") |
| clone | `bench/repos/typescript-go` (gitignored by `bench/.gitignore` `repos/`), `--depth 1 --branch v7.0.2` |
| binaries | `bench/repos/tsgo-bin/tsgo-base` (stock), `tsgo-wedge` (patches b + c), `go build ./cmd/tsgo`, go 1.26.3 |
| corpus | hafley-rxjs `dc3d699d`, `packages/`, read-only; its own typescript is 7.0.2 |
| client | `2_wedges/client.ts`, node v24.15.0 (type stripping), JSON-RPC over stdio |
| baseline | release `ryii` from this worktree, `--features cli,ts-checker,typespec` |

## Files

- `0_clone.sh`: clone, check the commit, build `tsgo-base`, apply patches, build `tsgo-wedge`.
- `1_map.md`: need -> entry point table.
- `2_wedges/b_lsp_batch.patch`, `2_wedges/c_api_batch.patch`, `2_wedges/client.ts`.
- `3_measure.sh`: one `/usr/bin/time -l` run per (wedge, question); writes `results.tsv`.

## Wedges

| wedge | Go lines added | Go files | protocol |
| --- | --- | --- | --- |
| a1 `lsp-noopen` | 0 | 0 | LSP, no `didOpen`; one `textDocument/definition` per site |
| a2 `api` | 0 | 0 | `tsc --api --async`: `updateSnapshot{openFiles}`, per file `getDefaultProjectForFile` + `getSymbolsAtPositions`, per alias `getAliasedSymbol`, per declaration file `getSourceFile` (binary AST decode for the name offset); relations: `getTypesAtPositions` per file + `isTypeAssignableTo` per pair |
| b `lsp-batch` | 163 | `checker/exports.go` +16, `ls/ryibatch.go` +131 (new), `lsp/server.go` +16 | LSP request `ryi/batch` |
| c `api-batch` | 163 | `checker/exports.go` +16, `ls/ryibatch.go` +131 (new), `api/session.go` +16 | API request `ryiBatch` after one `updateSnapshot{openFiles}` |

b and c share `ls/ryibatch.go`: for every `(uri, [Position])` it calls the language service's own
`ProvideDefinition` (same alias, overload and declaration-map behaviour as `textDocument/definition`), then per
program takes one checker and answers `GetSymbolAtLocation` / `GetTypeAtLocation` ids and the requested pairs
(`assignable`, `subtype`, `strict_subtype`, `identical`, `comparable`). `checker/exports.go` adds four exported
one-line wrappers over the existing unexported relation functions; no checker logic is copied.

Request shape (both channels):

```json
{"snapshot": 1, "definition": true, "types": false,
 "files": [{"uri": "file:///...", "positions": [{"line": 6, "character": 18}]}],
 "pairs": [{"source": 0, "target": 1, "relation": "assignable"}]}
```

Result: `{"files": [[{"definition": <LSP Location[]>, "symbol": id, "type": id}]], "pairs": [true|false|null]}`;
pair indexes count positions flattened in file order; a pair across two programs answers `null`.

## Measurements (single runs, `results.tsv`)

Wall/CPU/RSS cover the client (node) and the server; RSS is the largest single process.
`requests` counts client -> server requests (LSP `initialize` included, notifications excluded).

| wedge | question | wall_s | user_s | sys_s | max_rss_mb | requests | files_opened | answers | answers_equal |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ryii today | Route | 1.66 | 3.94 | 0.84 | 393 | | 4 | 11 | truth |
| lsp-open | Route | 0.33 | 0.99 | 0.41 | 316 | 15 | 6 | 11 | yes |
| a1 lsp-noopen | Route | 0.22 | 0.69 | 0.28 | 258 | 15 | 0 | 11 | yes |
| a2 api | Route | 0.21 | 0.76 | 0.25 | 255 | 28 | 6 | 10 | no |
| c api-batch | Route | 0.20 | 0.69 | 0.24 | 261 | 3 | 6 | 11 | yes |
| b lsp-batch | Route | 0.24 | 0.73 | 0.27 | 259 | 2 | 0 | 11 | yes |
| lsp-open | Signal | 2.63 | 7.91 | 2.06 | 1283 | 582 | 133 | 549 | truth |
| a1 lsp-noopen | Signal | 2.01 | 7.34 | 2.07 | 1339 | 582 | 0 | 549 | yes |
| a2 api | Signal | 0.64 | 2.99 | 0.82 | 817 | 829 | 133 | 187 | no |
| c api-batch | Signal | 0.97 | 3.35 | 0.83 | 1038 | 3 | 133 | 549 | yes |
| b lsp-batch | Signal | 1.56 | 7.06 | 1.97 | 1576 | 2 | 0 | 549 | yes |
| ryii today | Signal | 3.99 | 4.25 | 0.83 | 384 | | 134 | 188 | no |
| a2 api | relations | 0.11 | 0.34 | 0.08 | 136 | 27 | 4 | 20 | truth |
| c api-batch | relations | 0.10 | 0.37 | 0.08 | 138 | 3 | 4 | 20 | yes |
| b lsp-batch | relations | 0.10 | 0.34 | 0.08 | 138 | 2 | 0 | 20 | yes |
| LSP | relations | | | | | | | 0 | unreachable |

- Route: `graph --slow --callers packages/signals/src/5_Route.ts#Route packages`. ryii's checker span
  (`ts7_callers`) is 238 ms of its 1.66 s; its 4 opened files come from the trace. The client's text prefilter
  (`\bRoute\s*(<..>)?\(`) lists 6 files / 14 sites (comments included); the checker keeps the same 11 edges.
- Signal: `callers=Signal@packages/signals/src/2_Signal.ts`, added to show scale (133 files, 581 sites); truth is
  today's protocol (`lsp-open`), 549 edges (543 distinct file:line). ryii's own run returns 188 edges, all in
  `packages/signals` (checker span 2.65 s, 134 files asked). Distinct file:line: 357 truth edges are absent from
  ryii, 356 of them in other packages that import `@hafley66/signals`; 2 ryii edges are absent from truth (sites
  `Signal<{ ... Record<..> }>(` the client's prefilter regex skips). The definition reply for a cross-package site
  (`signal-grid/demo/1_everything.ts`) and an in-package site (`signals/src/10_slice.ts`) is the same 7 locations
  in `2_Signal.ts` in the client's LSP session; the 356 ryii misses are byte-identical to the 356 a2 misses
  (declarations under `packages/signals/dist/*.d.ts`, outside the supplied file set).
- a2 misses the edges whose declaration resolves through a project reference to `packages/signals/dist/*.d.ts`
  (`signal-grid/demo/main.ts:119` for Route; the same 356 cross-package file:line set for Signal): the API
  returns the `.d.ts` declaration and has no declaration-map mapping; the LSP path maps it back to `src/`
  (`internal/ls/definition.go:196`).
- relations: 20 ordered pairs over `DepthLimit`, `ProxyTreeDepth`, `AsyncStatus`, `Serializable`,
  `EndpointRequest` (type at the declaration name), relation `assignable`. True: DepthLimit<->ProxyTreeDepth,
  DepthLimit->Serializable, ProxyTreeDepth->Serializable, AsyncStatus->Serializable, EndpointRequest->Serializable.
  The Node script answers none: on typescript@7.0.2 it throws `TypeError: Cannot read properties of undefined
  (reading 'fileExists')` at `ts_checker.mjs:91`.
- `lsp-batch` vs `api-batch` on Signal: same `ProvideDefinition` calls; the LSP process spends 7.06 s user vs
  3.35 s and 1576 vs 1038 MB. Loading all files' projects through one `GetLanguageServicesForDocuments` call
  instead of per-file `GetLanguageService` measured 1.81 s / 1496 MB (not kept).

## Ranking

1. c `api-batch` (163 Go lines): one `updateSnapshot` + one `ryiBatch` per question; answers equal on both
   callers questions and the relation sample; Signal 0.97 s vs 2.63 s for today's protocol, 3 requests vs 582;
   reaches subtype / strict subtype / identical / comparable, which no stock channel exposes.
2. b `lsp-batch` (163 Go lines): same answers, 2 requests; Signal 1.56 s, highest CPU and RSS of the batch forms.
3. a1 `lsp-noopen` (0 lines): drops the per-file `didOpen` snapshot; Signal 2.01 s vs 2.63 s; still one request
   per site; no type queries, so the Node script stays.
4. a2 stock API (0 lines): identity, has_type, assignable and conforms (`getBaseTypes`) are reachable, one
   request per pair/handle; no subtype or identical; definitions differ from today's (10/11, 187/549).

What ryi changes to use c:

- A JSON-RPC session to `tsc --api --async --cwd ROOT` beside `1a_ts7_lsp_session.rs` (same Content-Length
  framing, no LSP initialize handshake), resolving the patched binary instead of the npm `bin/tsc` shim, since
  `ryiBatch` exists only in the patched build.
- `1g_ts7_resolve.rs::ask`: replace the `sync_document` loop and the per-site `textDocument/definition` loops (calls
  and JSX attributes) with `updateSnapshot{openFiles}` + one `ryiBatch{definition: true}`; each answer is the
  same LSP `Location[]` the existing `locations()` / `byte_at_lsp_position` code reads.
- `hafley_scm` `ts_checker.rs` + `ts_checker.mjs`: replaced by `ryiBatch{types: true, pairs}` for symbol/type
  identity, has_type and the relation rows. The Node script's type-structure rows (properties, signatures,
  parameters, type arguments, constraint, `typeToString`) are not in the batch; they need either the stock
  per-handle API methods or more fields in `ryibatch.go`.
- `1ga_ts7_callers.rs` keeps its CST prefilter; positions go out as LSP line/character as today.

## Cross-package Signal callers (fix/ts-cross-package-callers, 2026-10-05)

`traffic/` (gitignored except `deframe.ts`; regenerate locally) holds the LSP traffic of one ryii run before and after the fix (`ryi-{before,after}.{in,out}.jsonl`,
deframed by `traffic/deframe.ts`; `didOpen` texts elided; `window/logMessage` and diagnostics dropped) and the
client's trace (`client-npm.trace`, `lsp-open` against the npm `typescript@7.0.2` shim, 549 edges, same as `tsgo-base`).

- Both sessions get the same answers. A cross-package site (`boop-adapters/src/report-app/model.ts`, line 41)
  gets the six overload signatures of `Signal` in `packages/signals/src/2_Signal.ts` (0-based lines 35-40, mapped
  from `dist/2_Signal.d.ts` through its declaration map) and the type alias (line 4), never the implementation
  (line 41), which a `.d.ts` does not hold. An in-package site gets the implementation too.
- ryi's `CheckerDefs::target` has no definition span at an overload signature (no body), so every cross-package
  answer named no definition and the site was dropped. The same holds for a same-file call answered with one
  overload (`2_Signal.ts:18`, `toSignal`).
- Fix: when no answered location names a definition, ryi asks `textDocument/definition` at the answered
  signature's own name (cached per location); that answer lists the implementation.
- After: 627 edges (621 lines). Every client edge is in ryi's set. ryi's extra 78 lines are sites the client's
  text prefilter `\bSignal\s*(<[^>(]*>)?\(` never asks (nested generics or `(` inside the type arguments); none of
  them is in the client's asked set. `--callers Route` stays 11; `packages/signals` alone gains `2_Signal.ts:18`.


## Stock TSI tier parity (2026-10-05)

Same root: `tests/fixtures/tsi`; supplied file: `probe.ts` (542 bytes).
Old: the base commit `9d4f0dc6` Node driver with local TypeScript **5.9.3**.
New: the demand-scoped tier with the bundled, unmodified TypeScript **7.0.2** native executable.
The old capture was taken once before replacing the driver; the new capture was taken once
with `--witness --resolve --arms type --root tests/fixtures/tsi --ts-checker`.
Only facts witnessed by `checker_walk` count as new checker rows; syntax rows are excluded.
The new capture took 0.189 s wall time including the count extraction script; the logged tier
load was 72 ms and its API walk was 14 ms. No whole-corpus measurement was run.

| relation | rows_old | rows_new |
| --- | ---: | ---: |
| ts.interface | 1 | 1 |
| ts.mapped | 1 | 0 |
| ts.optional | 7 | 7 |
| ts.readonly | 3 | 0 |
| tsi.argument | 3 | 2 |
| tsi.assignable | 0 | 1 |
| tsi.callable | 4 | 4 |
| tsi.called | 3 | 2 |
| tsi.conforms | 2 | 0 |
| tsi.denotes | 4 | 11 |
| tsi.edge | 18 | 22 |
| tsi.has_type | 40 | 15 |
| tsi.input | 4 | 4 |
| tsi.name | 24 | 29 |
| tsi.origin | 20 | 16 |
| tsi.output | 4 | 4 |
| tsi.parameter | 4 | 4 |
| tsi.primitive | 2 | 4 |
| tsi.product | 7 | 8 |
| tsi.sum | 0 | 3 |
| tsi.symbol | 4 | 9 |
| tsi.type | 20 | 20 |
| TOTAL | 175 | 166 |

These counts compare each tier's documented demand policy. The old driver walked every
identifier and type-reference site; the new tier queried the 15 sites selected by the request.
The fixture has no tsconfig. TS5's driver uses its default compiler options; the stock LSP
inferred project adds null/undefined unions to optional property types.

Shape details retained from the former t_101 cases:

| shape | stock snapshot evidence | coverage limit |
| --- | --- | --- |
| Product fields | `tsi.product`, field `tsi.edge`, optional flags and typed targets | `ts.readonly` declaration modifiers are absent from the selected API responses |
| Generic call result | `Partial<User<number>>`, computed `User<number>` handle, `tsi.called` and `tsi.argument` | Some type-reference token positions return stock `any`; returned application handles carry the computed structure |
| Mapped type | Computed mapped product fields and seven `ts.optional` rows | `ts.mapped` key/constraint/template decomposition is absent from the selected per-type API |
| Callable input | Four `tsi.callable`, four `tsi.input`, four `tsi.output`, four `tsi.parameter` rows | No claim of exhaustive overload/constraint structure |
| Heritage conformance | Direct fixture `0_stock.ts` declares one class-extends `tsi.conforms` through `getBaseTypes` | The two TS5 `implements Mapper` conforms rows in `probe.ts` have no stock base-type answer |

Stock emits no subtype, strict_subtype, identical or comparable rows. Partial coverage claims
include diagnostics for these omissions. The direct demand test additionally asks both
assignability directions between Derived and Base, checks one passing pair, and verifies
symbol identity, UTF-16 positions, an unchanged process id and zero LSP opened documents.
The whole-stream fixture table explicitly checks that witness-off emits no semantic run or
semantic rows and that every id named by an argument is declared in that stream.

`4_stock_tier_parity.py` reproduces one run per tier without installs or builds and prints
`relation / rows_old / rows_new`; `5_stock_tier_parity.tsv` records this comparison. Example
from this crate's workspace:

```sh
python3 bench/labs/lab-20261005-tsgo-wedge/4_stock_tier_parity.py \
  --ryii "$CARGO_TARGET_DIR/debug/ryii"
```


Final crate gate (one background run, `KACHE_DISABLED=1`, features
`cli,ts-checker,go-checker`): unit targets 7/7 and 18/18; consolidated target
1,282 passed, 3 failed, 19 ignored in 172.34 s. Base failures: zero observed.
Branch failures: the graph-decline test still selected the retired JS driver;
the captured root help named `tsc`; the quality gate reported the relocated
static session cache and new free-function names. Corrections select `SPREFA_TSGO`
and `tier.tsgo`, pin the help label to `tsgo`, move the existing static-cache
allowance to its shared location, record the tier's public `answer`, and name
private helpers `lsp_position` / `collect_semantic_rows`.
The corrected decline behavior and captured help were checked directly against
the built binary. Final `cargo check --features cli,ts-checker,go-checker` passed after these corrections.
No second suite run was performed. Cargo's failed consolidated
target stopped later targets and doc tests.
