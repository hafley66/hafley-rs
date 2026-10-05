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
