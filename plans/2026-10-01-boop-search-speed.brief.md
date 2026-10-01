# Brief: `boop db search` takes 16 s on the real store

Worktree: `/Users/chrishafley/projects/hafley-rs/.boop-worktrees/fix/boop-search-speed`, branch
`fix/boop-search-speed`, base `main` `947ab611` (includes fix/boop-search-noise). Work in `$PWD`.

## Measured

- `boop db search fails_at --days 3 --limit 5` on `~/.agent/boop.db` (2.5 GB): 16.3 s with the
  merged build (read verbs no longer sync or drain). Scratch store: 25 ms.
- Target: under 1 s on the real store, same results.

## Goal

Find where the 16 s goes (query plan, full scan of turn text, per-row decode, window filter
applied late) and fix it. Candidate approaches must be researched before building: SQLite FTS5
(built in), trigram tokenizer, an index on the time column, pushing `--days`/`--harness` into SQL.
Write the candidate table (approach / cost / disk / write-path impact) in the brief's sibling
`plans/2026-10-01-boop-search-speed.plan.md` before coding. No hand-rolled index or tokenizer.

## Laws

- Read the real store read-only (`file:...?mode=ro` or a copy in scratch). Never write to
  `~/.agent/boop.db`. A schema change (new FTS table, index, migration) is a stop: report the plan
  with measured numbers on a scratch copy and wait.
- Integration tests: the real binary against a scratch store. Unit tests for pure code only.
- Rust builds: plain `cargo` (rcargo to the Spark). One cargo process at a time, no background
  builds. Profile settings in `Cargo.toml` stay.
- Scoped commits; no merge, no push, no install.

## Deliverable

```
status: done | blocked
sha: <HEAD>
files: <touched>
validation: <command + result line each>
timing: <real store before> -> <after>, same hit set (diff of ndjson)
next: <one action or "parent">
```

Stop on: a schema change or migration, a change to the write path, or scope change.
