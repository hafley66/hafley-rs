# Brief: lab-20261005-tsgo-wedge

Question: what is the least code to add to TypeScript 7 (the Go compiler, "tsgo") so ryi's
slow TypeScript tier gets everything it needs over ONE efficient channel, with the old Node
script (TypeScript 5 JS API driver) retired?

## What ryi needs from the TypeScript checker

| need | today | gap |
| --- | --- | --- |
| definition of a call site / JSX attribute / type reference | LSP `textDocument/definition`, one request per site (1g_ts7_resolve.rs, 1ga_ts7_callers.rs) | per-request round trips; `didOpen` builds a project per opened file (30.6 s seen when 1,524 files were opened) |
| references to a symbol | LSP `textDocument/references` (ts7_graph_target) | opens every file |
| type relations rows: assignable, conforms, subtype, equivalent, has_type, plus symbol/type identity | Node script over the TS 5 JS API (hafley_scm read/lang/ts_checker.rs + ts_checker.mjs) | no LSP method; the Node script crashes on TS 7.0.2 (`ts.sys` undefined) |
| batch: many positions in one request, no per-file project rebuild | none | |

## Steps

1. Source: fresh clone of the TypeScript 7 Go source into `crates/sprefa-extract/bench/repos/typescript-go`
   (gitignored). The user's checkout at `/Users/chrishafley/projects/typescript-go` has untracked work:
   read it if useful, never modify it. Its last commit is "Add closure notice and link to original repo":
   find where the Go compiler lives now (upstream may have moved, e.g. into microsoft/TypeScript) and
   check out the tag/commit matching the `typescript@7.0.2` npm package ryi uses
   (`crates/sprefa-extract/ts7/package.json`). Record the commit.
2. Map what already exists: the LSP server (`internal/lsp`, `internal/ls`), the new IPC/API surface
   (`internal/api`, `cmd/tsgo` flags such as `--api`), the checker entry points for relations
   (`internal/checker`: isTypeAssignableTo, isTypeSubtypeOf, getTypeAtLocation, getSymbolAtLocation,
   resolved signatures), and the project model (`internal/project`: why opening a file builds a project).
   Table: need -> existing entry point (file:line) -> reachable from LSP? from the API? -> cost.
3. Candidate wedges, smallest first. For each: the Go diff (lines), protocol shape, and a measurement.
   - (a) no patch: use the existing API/IPC mode if it already exposes checker queries; or LSP with
     workspace-level project (tsconfig) loading instead of per-file didOpen.
   - (b) one custom LSP request (e.g. `ryi/batch`) taking many (file, position, query) items and
     answering definition + type id + relation queries in one round trip from the already-built program.
   - (c) a custom API method on the existing IPC/API server, same batch shape.
   Prefer reusing an existing channel; a wedge must not fork the checker's logic.
4. Measure on hafley-rxjs `packages/` (read-only; ryi corpus): the `--callers` question for
   `packages/signals/src/5_Route.ts#Route` (11 sure edges today in 1.69 s, release ryii), plus one
   type-relation sample (assignability of 20 type pairs taken from `packages/signals`). For each
   wedge: wall, CPU, RSS, requests, files opened, answers equal to today's.
   Build tsgo with `go build` (Go 1.26.3 installed); keep builds out of the repo tree except bench/.
5. Do not change ryi product code. A small prototype client is fine inside the lab directory.

## Rules

- One agent; single measured runs; no sweeps; every command under 2 minutes except clone and Go builds.
- Repo rules: Rust/TypeScript only; reports in plans/, bench clones and DBs in crates/sprefa-extract/bench/;
  build vs buy: prefer an existing tsgo channel over a patch; a patch is the smallest diff that works.

## Net new files

`crates/sprefa-extract/bench/labs/lab-20261005-tsgo-wedge/`: `README.md` (commits, versions, map table),
`0_clone.sh`, `1_map.md` (need -> entry point table), `2_wedges/` (one patch file per wedge + a
prototype client), `3_measure.sh`, `results.tsv`. Results appended to this brief.

## Deliverable

Branch `lab/20261005-tsgo-wedge`; commit the lab. Report: the map table, each wedge with diff size
and measurements, and a ranked recommendation: the least-code wedge that retires the Node script and
makes the slow TypeScript tier batch, with what ryi would change to use it.

## Results (2026-10-05)

Lab: `crates/sprefa-extract/bench/labs/lab-20261005-tsgo-wedge/` (README.md holds the full table and notes).

- Source: microsoft/typescript-go is closed; the Go compiler lives in microsoft/TypeScript under `tsc/`.
  Tag `v7.0.2` = `1e4744d68260a7cb91b62b12edc3f6a2187faaf1` (`tsc/internal/core/version.go` = 7.0.2); the npm
  package's `gitHead` 2bd066d8 is not fetchable.
- Existing channels: `tsc --api --async` is a JSON-RPC (Content-Length framed) checker API with batched
  `getSymbolsAtPositions` / `getTypesAtPositions`, `isTypeAssignableTo`, `getBaseTypes`, type-structure getters,
  and `updateSnapshot{openFiles}` loading many files in one snapshot. It has no definition method (node handles
  plus binary AST decode, no `.d.ts` -> source mapping) and no subtype / identical / comparable. The LSP has
  definition and references and no type queries. `didOpen` runs one snapshot update per file
  (`internal/project/session.go:295`); an unopened file is answered from disk.

| wedge | Go lines | question | wall_s | user_s | max_rss_mb | requests | files_opened | answers_equal |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ryii today | 0 | Route callers | 1.66 | 3.94 | 393 | | 4 | truth (11) |
| a1 lsp-noopen | 0 | Route callers | 0.22 | 0.69 | 258 | 15 | 0 | yes |
| a2 stock api | 0 | Route callers | 0.21 | 0.76 | 255 | 28 | 6 | no (10) |
| b lsp `ryi/batch` | 163 | Route callers | 0.24 | 0.73 | 259 | 2 | 0 | yes |
| c api `ryiBatch` | 163 | Route callers | 0.20 | 0.69 | 261 | 3 | 6 | yes |
| lsp-open (today's protocol) | 0 | Signal callers | 2.63 | 7.91 | 1283 | 582 | 133 | truth (549) |
| a1 lsp-noopen | 0 | Signal callers | 2.01 | 7.34 | 1339 | 582 | 0 | yes |
| a2 stock api | 0 | Signal callers | 0.64 | 2.99 | 817 | 829 | 133 | no (187) |
| b lsp `ryi/batch` | 163 | Signal callers | 1.56 | 7.06 | 1576 | 2 | 0 | yes |
| c api `ryiBatch` | 163 | Signal callers | 0.97 | 3.35 | 1038 | 3 | 133 | yes |
| a2 stock api | 0 | 20 assignable pairs | 0.11 | 0.34 | 136 | 27 | 4 | truth (20) |
| b lsp `ryi/batch` | 163 | 20 assignable pairs | 0.10 | 0.34 | 138 | 2 | 0 | yes |
| c api `ryiBatch` | 163 | 20 assignable pairs | 0.10 | 0.37 | 138 | 3 | 4 | yes |

Ranking: c, b, a1, a2. c and b share `ls/ryibatch.go` (+131, calls `ProvideDefinition` per position, then one
checker per program for symbol/type ids and `assignable`/`subtype`/`strict_subtype`/`identical`/`comparable`) and
`checker/exports.go` (+16, four exported wrappers); c adds 16 lines to `api/session.go`, b 16 lines to
`lsp/server.go`. ryi's change for c: an API JSON-RPC session to the patched binary, `1g_ts7_resolve.rs::ask`
sends `updateSnapshot` + one `ryiBatch` instead of `didOpen` + per-site definitions, and `ryiBatch{types, pairs}`
replaces `ts_checker.rs`/`ts_checker.mjs` for identity, has_type and relation rows; the Node script's
type-structure rows still need the stock per-handle getters or more batch fields. Side finding: ryii's Signal
callers run drops 356 cross-package edges, the same set the stock API misses (declarations under
`packages/signals/dist/*.d.ts`).
