# ryi TS fix: workspace package imports resolve, including packages from hafley-tsp (plan row D2, case dogfood/ts/D02.sh)

Repo hafley-rs, crate crates/sprefa-extract (its own workspace root). Base: branch integrate/ryi-ts.
Read first: REPORT.md section "D2 workspace imports: stopped on case/plan contradiction" (commit e165815a, branch feature/ryi-ts-fix-workspace-imports):
it classifies all 12 unresolved pairs and names the resolver lines in crates/hafley_scm/src/read/lang/ts_resolve.rs.

## User decision 2026-10-02
@hafley66/alloy-rs is a workspace package: its source is /Users/chrishafley/projects/hafley-tsp/packages/rust (package.json name @hafley66/alloy-rs,
exports ".", "./emitter", "./adapters" -> ./dist/...). The copy installed in hafley-rxjs node_modules has empty source maps. D02 resolves it to hafley-tsp source; no external-package exemption.

## Task
1. The 10 corpus-internal pairs: fix the resolver per the REPORT causes (package root index.ts without src/; css assets selected from the conditional export map's
   runtime target and mapped to package/src/<file>.css as a fact even when css files are not extracted inputs).
2. alloy-rs: extend dogfood/ts/0_corpus.sh to add a detached read-only hafley-tsp worktree at 8f679b1 beside the corpus ($CORPUS.tsp), same pattern as the rxjs worktree.
   D02.sh passes it as an extra input root. The resolver maps a bare @scope/name specifier to the input root whose package.json name matches, then applies that
   package's export map and the dist->src fallback. Never write into /Users/chrishafley/projects/hafley-tsp itself.
3. D02.sh: only the extra root changes; the required-resolution set stays every @hafley66/ specifier.

Rules:
- CODE COMPLETE ONLY. Do not run cargo build, cargo test, cargo check, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel (resolve regression in hafley_scm, cleave, help/quality gate). Keep diffs inside ts_resolve.rs and the dogfood files.
- No hand-written files labelled generated. No `boop beep scream`. No push. No Python.
- Commit per change; subject starts with D2; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section: per pair the change, and the exact command for the coordinator to verify.
