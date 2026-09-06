# Lane: codex per-turn cwd into write_turn; move the lane report off the root (hafley-rs)

Favor plain code. If reality deviates from this brief, STOP and write REPORT.md at `plans/2026-09-05-turn-cwd-codex.REPORT.md` (never at the worktree root) describing the deviation.

Base: 42361cc (branch feature/turn-cwd, already landed: `agent_turn.cwd_id`, `v_turn_cwd`, schema v27, claude path). Read `issues/boop-turn-cwd/item.md`; criterion 2 is the one left unticked.

## Files you own (inside $PWD)
- crates/boop-store/src/ident.rs: `write_turn` (:2941) gains `cwd: Option<&str>` as its last parameter and passes it to `add_turn` instead of `None`.
- crates/boop-harness/src/harness/codex.rs: in its `project_line`, keep the latest `turn_context.payload.cwd` (the read at :745 already extracts it) in the walk state and pass it to every `write_turn` call (:948, :961, :968, :979, :996). Every other `write_turn` caller in the workspace (`grep -rn 'write_turn(' crates`) passes `None`.
- crates/boop-harness/tests/fixtures/transcripts/codex/ (or wherever existing codex fixtures live; `ls crates/boop-harness/tests/fixtures`): a codex transcript with two `turn_context` records carrying different `cwd` values and one message after each. Test in the codex test module pins two distinct `cwd_id` values across the projected turns, reading through `v_turn_cwd`.
- REPORT.md at the root is a tracked file the previous lane overwrote (it was the soopy source-identity report at 29da6e8). Run `git mv REPORT.md plans/2026-09-05-turn-cwd.REPORT.md && git checkout 29da6e8 -- REPORT.md`. Put your own validation output in `plans/2026-09-05-turn-cwd-codex.REPORT.md`.
- issues/boop-turn-cwd/item.md: tick criterion 2, append to Tests Run.

Do not touch any other file.

## Rules
1. One commit, subject exactly: `boop-harness: codex turn cwd from turn_context into write_turn; lane report moves under plans/`.
2. Banned identifiers: provenance, substrate, load-bearing, regime.
3. Known pre-existing failure, report and leave: `ident::tests::schema_rows_lists_views_and_join_keys`.

## Validation (paste into the plans/ report)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/turn-cwd
cargo test -p boop-harness 2>&1 | grep -E '^test result|FAILED'
cargo test --workspace 2>&1 | grep -E '^test result|FAILED'
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
git status --short; git log --oneline -1; ls REPORT.md plans/2026-09-05-turn-cwd.REPORT.md
```
