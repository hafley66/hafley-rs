# REPORT: per-turn cwd on agent_turn

## Files changed
- `crates/boop-store/src/ident.rs`: `agent_turn.cwd_id INTEGER`, `v_turn_cwd` view (COALESCE fallback), schema v27 migration, `add_turn` gains `cwd: Option<&str>`, `project_line` passes each claude record's `cwd`, three tests.
- `crates/boop-store/tests/fixtures/claude-cwd.jsonl` (new): claude transcript with a `cd` mid-session.
- `issues/boop-turn-cwd/item.md`: acceptance boxes, status fixed, Tests Run.

## Test counts (before -> after)
- Before: `cargo test -p boop-store` = 144 passed, 1 failed
- After:  `cargo test -p boop-store` = 147 passed, 1 failed
- The 3 new tests all pass. The 1 failing test (`schema_rows_lists_views_and_join_keys`) fails identically on the base commit: it asserts a view is listed among `schema_rows()`, which returns only tables. It lives in `query.rs`, which is out of scope. Not touched.

## Deviations
1. Codex per-turn cwd is not wired. Codex transcripts are projected by `crates/boop-harness/src/harness/codex.rs` (a separate `project_line`), which calls `store.write_turn(...)`; `write_turn` is the generic writer and passes `cwd = None`. boop-harness is out of scope (do not touch), so codex turns fall back to the session cwd via the view's COALESCE. Acceptance criterion 2 is left unticked. kimi and opencode are session-level only and pass `None` by design.
2. `cargo test -p boop-store` is not fully green: one pre-existing failure (see above), unrelated to this change.
3. `cargo build -p boop` emits one pre-existing dead-code warning in `crates/boop/src/cli/debug.rs:186` (`run_host`), unrelated to this change.
4. The v27 `ALTER TABLE agent_turn ADD COLUMN cwd_id` is guarded by a `pragma_table_info` column-existence check rather than a bare ALTER, so re-entered migrations stay idempotent (matching the v14/v16/v20 pattern). This is required because the store's migration tests rewind `user_version` and reopen a fresh (already v27) store.
