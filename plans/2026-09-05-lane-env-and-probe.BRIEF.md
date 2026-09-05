# Lane: `lane create --env` flag and the start-ack probe collision (hafley-rs)

Favor plain code. If reality deviates from this brief, STOP and write REPORT.md at the worktree root describing the deviation; do not improvise.

## Issues (read both in $PWD first)
- issues/lane-create-env-flag/item.md
- issues/boop-probe-collision/item.md

## Files you own (inside $PWD, your worktree)
- crates/boop/src/cli/job.rs: `LaneArgs` gets `#[arg(long = "env", value_name = "KEY=VAL")] env: Vec<String>`; parse `KEY=VAL` at the arg seam (reject a value with no `=` with a clap error); thread it to the spawn. Do not touch `shell_quote` at :1148 or `completion_recipient`.
- crates/boop-proc/src/supervise.rs: the spawn takes the env pairs and sets them on the harness child process; the start-ack probe gate (`start_ack_pending`, :690, :726, :746, :812) never sends `START_ACK_PROMPT` while a turn is in flight and never sends it on a resume; add the guard the issue's first suggestion names ("skip when a turn is active") and a test that pins it.
- crates/boop-store/src/bus.rs `Route` (:26 area) if the spawn env must be recorded on the route: add `env: Vec<String>` with `#[serde(default)]`, nothing else in that file.
- crates/boop/tests/: one integration test that `--dry-run` prints the env pairs on the `cmd:` line.
- The two issue files: tick met boxes, `status: fixed`, `## Tests Run` with pasted output.

Do not touch any other file.

## Rules
1. `--env` is repeatable; `--env A=1 --env B=2`. The `--dry-run` `cmd:` line shows each pair.
2. `CARGO_TARGET_DIR` is the motivating example; no special-casing of that name.
3. Two commits, subjects exactly:
   - `boop: lane create --env KEY=VAL, repeatable, on the spawn and the dry-run line`
   - `boop-proc: start-ack probe skips a lane with a turn in flight`
4. Banned identifiers: provenance, substrate, load-bearing, regime.

## Validation (run all, paste output into REPORT.md)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/lane-env
cargo test -p boop-proc 2>&1 | tail -15
cargo test -p boop 2>&1 | tail -15
cargo clippy -p boop -p boop-proc --all-targets -- -D warnings 2>&1 | tail -5
cargo run -p boop -- beep lane create --branch feature/x --brief /tmp/x.md --preset flash4 --env A=1 --env B=2 --dry-run 2>&1 | grep cmd:
git log --oneline -2
```
Known env-only failure you may see and must report but not fix: `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung`.
Write REPORT.md at the worktree root.
