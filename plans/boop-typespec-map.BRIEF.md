# boop-typespec-map (read-only survey, sol6)

Goal: write ONE file `plans/boop-typespec-map.REPORT.md` and commit it. Edit nothing else.

## Direction (user, 2026-09-25)
boop's CLI, crates' shared types, API (IPC/RPC/HTTP), and sqlite schema should all be authored in TypeSpec and emitted.
This lane maps the current hand-written surfaces onto the existing TypeSpec machinery. It writes no .tsp and moves nothing.

## Read first, completely
- ~/projects/claude-research/skills/typespec/SKILL.md, then ~/projects/claude-research/tsp-arch/SESSION.md and the tsp-arch files it names.
- ~/projects/hafley-tsp/packages/*: for each package, what it emits (read its README/src entry), its pinned compiler version,
  and one compiled example if present.

## Surfaces to inventory (read only)
1. CLI: `crates/boop` clap definitions (every subcommand, arg, flag, env var). Command: count via the clap derive structs.
2. Shared types: `crates/boop-types`, `crates/boop-store/src/{rows.rs,query.rs,event.rs}` and every serde struct crossing a process boundary.
3. sqlite: every CREATE TABLE / migration in boop-store (tables, columns, types, indexes, uniqueness).
4. API: instant `ipc/commands.json` (boop_*/squares_*/harness_* rows), instant src-tauri commands, instant `src-tauri/src/serve/rpc.rs`,
   boop mail/bus message shapes, `boop db` output JSON, and hafley-rxjs `plans/boop-props-map.REPORT.md` Table A.
5. Existing codegen: instant `scripts/generate-native.mjs` and `generate-api.mjs` (inputs, outputs), sprefa-extract/schema/*.tsp in this repo (how it is compiled and consumed).

## Report shape (tables only)
A. surface | item | current source file:line | current source of truth (hand Rust / json / sql) | consumers
B. hafley-tsp emitter | emits | covers surface rows (A ids) | gap
C. surface rows with no emitter today (A ids) | what emitter is missing (e.g. clap CLI, Endpoint TS client)
D. naming/casing conflicts across surfaces (same data, different field names) | file:line each
E. proposed .tsp file layout (numeric-prefixed, dependency order) | declares | emitted to (crate/package path)
F. migration order: step | replaces hand-written file(s) | gate that proves equality (e.g. generated struct == existing serde JSON on fixtures)
G. commands run and output line counts.

## Validation
`test -s plans/boop-typespec-map.REPORT.md && git log -1 --format=%s`
## Commit
Subject exactly: `plans: boop typespec map report`
Receipt: status / sha / files / validation / next.
