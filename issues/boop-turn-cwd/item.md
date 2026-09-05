---
created: 2026-09-05
updated: 2026-09-05
type: feature
status: open
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

- [ ] Fixture: a claude transcript with a `cd` mid-session projects two
      distinct `cwd_id` values across its turns; test pins both.
- [ ] Codex fixture with `turn_context` projects the per-turn cwd.
- [ ] `boop db "select ... from v_turn_cwd"` (or the helper) returns the
      turn cwd with fallback; test covers a NULL row.
- [ ] Schema v27 migrates the live db in place; `cargo test -p boop-store` green.

## Follow-up (not this issue)

instant jump palette and refresolve read the turn cwd instead of the session cwd.
