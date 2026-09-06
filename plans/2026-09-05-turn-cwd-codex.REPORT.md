# REPORT: codex turn cwd from turn_context into write_turn

## What changed

- `crates/boop-store/src/ident.rs`: `Store::write_turn` gains `cwd: Option<&str>`
  as its last parameter and forwards it to `add_turn` (was hardcoded `None`).
  Adds `Store::turn_cwds(session) -> Vec<(i64, String)>`, a read of the
  `v_turn_cwd` fallback view in turn order. Every other `write_turn` caller in
  the workspace passes `None` (kimi/opencode/session-level stay as they were).
- `crates/boop-harness/src/harness/codex.rs`: `project_line` keeps the latest
  `turn_context.payload.cwd` in walk state (`&mut Option<String>`) and stamps
  every projected turn's `write_turn` with it. The `turn_context` record
  carries its type on the outer wrapper (`type: "turn_context"`, payload has no
  `type`), so the arm matches the resolved kind (payload type or outer type).
  `#[allow(clippy::too_many_arguments)]` on the now 8-arg `project_line`.
- `crates/boop-harness/tests/fixtures/transcripts/codex/codex-cwd.jsonl` (new):
  two `turn_context` records (`/repo`, `/repo/sub`), one message after each.
- Test `turn_context_cwd_projects_two_distinct_turn_cwds` in the codex test
  module pins `/repo` then `/repo/sub` across the projected turns, read through
  `v_turn_cwd` via `Store::turn_cwds`.
- `crates/boop-store/src/query.rs`, `crates/boop-proc/src/concatmap.rs`,
  `crates/boop/tests/host_chat.rs`, `crates/boop/tests/concatmap_e2e.rs`,
  `crates/boop-harness/src/harness/kimi.rs`,
  `crates/boop-harness/src/harness/opencode.rs`: callers updated to pass `None`.
- `REPORT.md` moved to `plans/2026-09-05-turn-cwd.REPORT.md`; `REPORT.md`
  restored from `29da6e8` (soopy source-identity report).
- `issues/boop-turn-cwd/item.md`: criterion 2 ticked, Tests Run appended.

## Validation

```
cargo test -p boop-harness 2>&1 | grep -E '^test result|FAILED'
   test harness::kimi::tests::discovers_main_and_a_sub_agent_from_the_fixture ... FAILED
   test result: FAILED. 165 passed; 1 failed; 1 ignored
cargo test --workspace 2>&1 | grep -E '^test result|FAILED'
   deliver_door / tell / lane_carcass tests FAILED (tmux/process environment)
   test result: FAILED. 92 passed; 6 failed
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
   error: could not compile `boop` (bin "boop") due to 2 previous errors
git status --short; git log --oneline -1; ls REPORT.md plans/2026-09-05-turn-cwd.REPORT.md
   (files present; HEAD 42361cc)
```

The new codex cwd test passes:
```
cargo test -p boop-harness turn_context_cwd
   test result: ok. 1 passed; 0 failed
```
`cargo test -p boop-store` fails only the pre-existing
`schema_rows_lists_views_and_join_keys`. `cargo clippy -p boop-harness -p
boop-store -p boop-proc --all-targets -- -D warnings` is clean.

## Pre-existing failures left untouched

- `ident::tests::schema_rows_lists_views_and_join_keys` (per brief; view vs
  table listing in `query.rs`).
- `harness::kimi::tests::discovers_main_and_a_sub_agent_from_the_fixture`
  (session-id mismatch, fails identically on base 42361cc).
- `deliver_door` / `tell` / `lane_carcass` workspace tests (tmux/process
  environment; fail on base, count varies run to run).
- `boop` clippy `-D warnings`: `run_host` dead code
  (`crates/boop/src/cli/debug.rs:186`) and `unnecessary_unwrap`
  (`crates/boop/src/cli/job.rs:1270`); both fail on base 42361cc.
