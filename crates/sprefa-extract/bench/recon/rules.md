# Recon rules
- READ ONLY on ~/projects/hafley-rs (the repo is at ~/projects/hafley-rs/.boop-worktrees/claude-375, branch claude-375-g, = origin/main). Never edit it, never commit.
- Write only inside your current directory (downloads, clones, scratch, and the final REPORT.md).
- Read hafley-rs code with ryi / ryii (`ryi query`, `ryi graph`; `--help` first). Known bug: `#match?` outside an alternation `[...]` is ignored.
- Cite every claim as path:line or URL. Tables: one value per cell, full-word column names.
- Final answer = the report (it is saved as REPORT.md).
- DO NOT STOP AT THE FIRST WALL. If a dependency is missing, install it; if a baseline has errors, fix the environment or pick another target; if a tool fails to start, read its docs and logs and retry. A number measured on a broken environment is not a result.
- The report MUST end with a section `## Walls and caveats`: one row per thing that went wrong or was worked around (what, where, effect on the numbers, fixed yes/no). An empty section means nothing went wrong. Numbers affected by an unfixed wall are marked in their table.
- Instrumentation you add (spans, profiler hooks, memory fields) is written against `crates/hafley-observe` and delivered as `hafley-observe.patch`. Research existing crates first and cite them; no custom allocator or profiler when a crate exists.
- Comparative tests (tool vs oracle vs rival): use or extend ryi-bench in ~/projects/hafley-rs/crates/sprefa-extract/bench (once it exists). Do not write a new harness; deliver new targets/adapters as a patch against it.
