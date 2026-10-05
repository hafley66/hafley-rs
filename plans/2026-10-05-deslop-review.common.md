# Deslop review, read-only (2026-10-05)

You are a reviewer. Do NOT edit, commit, build, or run tests. Reading + `git` + `ryii query`/`graph`
(binary: /Users/chrishafley/.cache/boop/lanes/fix-ryi-dogfood-defects/target/debug/ryii if present) only.

Code under review = everything written today:
- main: `git -C /Users/chrishafley/projects/hafley-rs log -p 9146332d^..9d4f0dc6`
- branch fix/ryi-dogfood-defects: worktree /Users/chrishafley/projects/hafley-rs/.boop-worktrees/fix/ryi-dogfood-defects, range 9d4f0dc6..HEAD
- branch feature/ryi-tsgo-tsi-tier: worktree /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-tsgo-tsi-tier, range 9d4f0dc6..HEAD
(branches are still moving; review HEAD at the time you read, and record the hash.)

Repo rules that define slop here: hafley-rs CLAUDE.md (one implementation per concern; no workarounds,
fallback heuristics or text scans; abstain with a reason; new work in small numbered files; tests are
whole-output snapshots, no per-test duplicated setup), ~/AGENTS.md ("General JS global state" and
testing sections apply to Rust too: no one-line wrappers around array methods, no N+1 getters,
no toBeDefined-style weak asserts).

Slop to find:
1. duplicated logic (same concern implemented twice, incl. across the two branches)
2. dead code, unused params/fields, branches that cannot fire
3. format-then-parse round trips, stringly identity where a span/id exists
4. silent fallbacks (warn+continue, unwrap_or_default hiding an error, empty result instead of abstain)
5. comments narrating code, citing history/tickets/v5, or restating names
6. wrappers that add nothing; types/traits with one use
7. weak or redundant tests; snapshots that do not carry the claim; committed logs/debris
8. new gate allowlist entries and why they exist

Output: one file at the path given in your scope. Table per finding: file:line (at the hash you read),
category (1-8), what, smallest fix, lines it would delete. Sort by lines deletable, descending.
No praise, no summary prose. Count findings at the end.
