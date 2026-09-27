---
created: 2026-09-05
updated: 2026-09-05
type: feature
status: fixed
priority: high
epic: boop-process
---

# Per-turn cwd on agent_turn so relative refs resolve against the turn's directory

## Description

`agent_session.cwd_id` holds one cwd per session (`upsert_session_row`,
`crates/boop-store/src/ident.rs:924`). A claude session changes directory
whenever a Bash tool runs `cd`; every claude jsonl record carries `cwd`
(this session: 287 records at hafley-rs, 30 at a worktree). Codex carries
`turn_context.payload.cwd` per turn. Kimi and opencode are session-level only.
tmux `pane_current_path` does not follow the harness's chdir.

When an assistant turn says `crates/boop/src/cli/job.rs:1148`, the reader
(jump palette, refresolve, favorites) resolves it against the session cwd and
misses when the turn ran elsewhere.

## Expected

- `agent_turn.cwd_id INTEGER` (nullable, `dict_cwd`), schema v27 `ALTER TABLE`.
- `add_turn` (ident.rs:951) takes `cwd: Option<&str>`; `project_line`
  (ident.rs:2994) passes the record's `cwd` for claude and the turn's
  `turn_context.payload.cwd` for codex; kimi/opencode pass `None` and readers
  fall back to `agent_session.cwd_id`.
- A view or query helper `turn_cwd(session_id, turn) -> cwd` with the fallback
  applied in SQL (`COALESCE(t.cwd_id, s.cwd_id)`).
- Existing rows: NULL, fallback covers them. No backfill scan on migrate.

## Acceptance Criteria

- [x] Fixture: a claude transcript with a `cd` mid-session projects two
      distinct `cwd_id` values across its turns; test pins both.
- [x] Codex fixture with `turn_context` projects the per-turn cwd.
- [x] `boop db "select ... from v_turn_cwd"` (or the helper) returns the
      turn cwd with fallback; test covers a NULL row.
- [x] Schema v27 migrates the live db in place; `cargo test -p boop-store` green.

## Tests Run

```
cargo test -p boop-store 2>&1 | tail -20
   test result: FAILED. 147 passed; 1 failed (pre-existing schema_rows_lists_views_and_join_keys)
cargo clippy -p boop-store --all-targets -- -D warnings 2>&1 | tail -5
   Finished `dev` profile (no warnings)
cargo build -p boop 2>&1 | tail -3
   Finished `dev` profile (1 pre-existing dead-code warning, crates/boop/src/cli/debug.rs)
git log --oneline -1
   29da6e8 issues: boop-turn-cwd; plans: turn-cwd, lane-env-and-probe, tiny-cleanup-1 lane briefs
```

The one failing test (`schema_rows_lists_views_and_join_keys`) fails identically on
the base commit before this change: it asserts a view (`v_usage_cost`) is listed
among `schema_rows()`, which only returns tables. It lives in `query.rs`, out of
scope for this issue. Not touched.

Codex lane (boop-harness: codex turn cwd from turn_context into write_turn):
```
cargo test -p boop-harness 2>&1 | grep -E '^test result|FAILED'
cargo test --workspace 2>&1 | grep -E '^test result|FAILED'
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
```
- `write_turn` gains `cwd: Option<&str>` (passes through to `add_turn`); every
  other caller passes `None`.
- `codex::project_line` tracks the latest `turn_context.payload.cwd` in walk
  state and stamps every projected turn; new fixture
  `tests/fixtures/transcripts/codex/codex-cwd.jsonl`; test pins `/repo` then
  `/repo/sub` through `v_turn_cwd` via new `Store::turn_cwds`.



## Follow-up (not this issue)

instant jump palette and refresolve read the turn cwd instead of the session cwd.
