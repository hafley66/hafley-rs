# hafley-rs agent rules

## Tabular results
- Any table with more than ~10 rows or more than 5 columns goes to a browser grid, not chat:
  `python3 scripts/bench_grid.py` (jQuery DataTables, data inlined, per-column filters) writes
  `plans/bench-grid.html`; open it. Copy the script's pattern for other SQLite sources.
- Tables are 1NF: one value per cell, one column per metric. Never pack two metrics into one cell
  ("0.98 / 1.0", "2/2 sites; 1/1 files").
- Column names are full words (`file_recall`, `targets`), never single letters (R, P, n).

## Tool evaluation scope
- Rust and TypeScript only until ryi and its rivals are proven on those two. No Kotlin, Go or
  Python runs, fixtures, or truth indexes until the user lifts this.

## Comparative tests (user-set 2026-09-29)
- Every comparison of ryi against oracles or rivals runs through `ryiii` (dev-tool bin; bench data in `crates/sprefa-extract/bench/`).
  Labs add repos, targets or adapters there; they do not write their own harnesses, runners or scoring scripts.

## Delegation
- Reading and recon fan out in parallel: codex `gpt-6-luna` high, read-only, reports under
  `plans/recon/` in the repo, no worktree.
- No work output in cache folders (`~/.cache`, `~/Library/Caches`, `/tmp`), user-set 2026-09-30:
  reports go to `plans/`, bench clones and databases to `crates/sprefa-extract/bench/`.
- Writing is serial in ONE worktree checkout folder: codex `gpt-6.1-sol` medium (boop preset `sol61-med`).
  One writer at a time; merge to main before the next writer starts. Do not create per-task worktrees.
- The coordinator reviews every writer's diff before merge.
- Test gates are exempt from the 2-minute command cap (user-set 2026-10-05): a lane runs the
  full `cargo test --features cli --no-fail-fast` once, in the background, and reports failures
  split into base and new. Targeted gates are never capped and rerun.
- Briefs state the generic design (shared join, shared lift, shared trait) before any
  language- or framework-specific case (user-set 2026-10-05). A special case lands only as
  data or a fixture over the generic mechanism.
- Tests are min code, max information (user-set 2026-10-05): one table-driven test per concern
  over a fixture directory, whole-output snapshot, no per-case test functions. A new case is a
  fixture file or a table row, not a new `#[test]`. Delete a test whose claim another snapshot
  already carries.

## One implementation per concern (user-set 2026-09-28)
- Exactly one implementation of each concern (module resolution, name binding, rename sites,
  type edges). Fast and slow tiers share it; they never fork a second copy.
- Rust resolution is rust-analyzer as a library (`ra_ap_*`, feature `rust-checker`). It is part of
  the default `cli` build.
- Rust uses one rust-analyzer module-place provider for graph, rename, cleave and move.
  Names loads workspace module maps without caller-body inference; Types adds inference.
  Method calls abstain `needs_types` in Names. No edit arm keeps its own module or `#[path]` reader.
- Code stays tight and legible (user-set 2026-09-28). New work goes in a new small numbered file.
  Never grow a large file (over ~1000 lines) with new work; split it when you touch it.
- hafley-observe is the single home for tracing, profiling and memory instrumentation (user-set 2026-09-29).
  Any lab or writer that adds instrumentation lands it in `crates/hafley-observe` in the same cycle. A patch left in a lab folder is not done.
  Build vs buy applies: use existing crates (e.g. `tracking-allocator`) before a custom implementation.
- No workarounds or fallback heuristics. When the proper engine is unavailable, the command
  abstains or errors with the reason; it does not guess with text scans or a parallel resolver.
