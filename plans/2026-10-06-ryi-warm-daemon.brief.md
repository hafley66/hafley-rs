# Brief: warm daemon answers fast; land two old ryi branches

Base: origin/main. Decisions: plans/2026-10-04-rust-resolution-unify.plan.md (5: daemon module-place
provider; 8: cache key = manifest content ids). CLAUDE.md rules apply (generic first, tests as
fixture tables, one implementation per concern).

## Measured (2026-10-06, release, hafley_scm, `graph --callers 0d_flow.rs#flow_edges crates/hafley_scm`)
| path | wall |
|---|---|
| ryii one-shot | 6.0-6.3 s |
| ryi client, daemon warm | 4.3 s |
| --fast-receivers written, warm | 15.8-16.2 s |
Trace (one-shot): stage index_rust_modules dominates (Names load + module places per file, 196 files),
then project_facts. Warm Names module query measured 0.06 ms in isolation.

## Work, one commit each, failing measurement first
0. Land feature/ryi-bare-hateoas (3 commits: suggestions when no paths) and the two commits on
   chore/the-gang-edits (bound daemon worker memory, keep daemon fanout): rebase onto main, fix
   conflicts, their tests pass. Skip any commit that main already covers; say which.
1. Trace one warm daemon request (HAFLEY_TRACE; trace files get large, filter spans with grep, do not
   json.load the whole file). Name where the 4.3 s goes.
2. Daemon keeps per workspace (key = manifest content ids): the Names host, module places, and the
   written-receiver lowering cache. File edits update the host; no reload.
3. Module places for N files in one batched query, not one per file.
4. Extracted facts reused across requests by file content id where they are not already.
Target: warm fast `--callers` on hafley_scm under 0.5 s; warm written under 1 s; cold no slower than
today. Table: path, cold_s, warm_s, before/after, single runs.

## Gates
Full suite once on the final commit (sprefa-extract cli,ts-checker; hafley_scm rust-checker),
counts in the report; coordinator pushes from it. No merge, no push.
