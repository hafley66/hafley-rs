# Lane: `boop beep lane create --env KEY=VAL` (hafley-rs)

Favor plain code. If reality deviates from this brief, STOP and write `plans/2026-09-05-lane-create-env.REPORT.md` (never a root REPORT.md) describing the deviation.

Read `issues/lane-create-env-flag/item.md` and the previous lane's analysis at `.boop-worktrees/fix/lane-env-and-probe/REPORT.md` is NOT in your tree; its findings are restated below. The probe-collision issue is out of scope for this lane.

## Where the pieces live (verified by the previous lane)
- The clap derive for `lane create` is `LaneCmd::Create` at crates/boop/src/main.rs:931-1026. `LaneArgs` (crates/boop/src/cli/job.rs:801) is a plain struct built from it at job.rs:1500-1567, and again at job.rs:1410 (`run_fork`).
- Env reaches the harness child through the `env_stamp` prefix on the supervisor command: `spawn_env_stamp` at job.rs:192 builds it, `supervisor_command` at crates/boop-harness/src/harness.rs:500-503 prefixes it to `boop beep lane run ...`, and the harness child inherits it. supervise.rs never touches the child `Command`; do not edit it.

## Files you own (inside $PWD)
- crates/boop/src/main.rs: `#[arg(long = "env", value_name = "KEY=VAL")] env: Vec<String>` on `LaneCmd::Create`, with a clap `value_parser` that rejects a value with no `=` or an empty key.
- crates/boop/src/cli/job.rs: `LaneArgs.env: Vec<(String, String)>`; both construction sites; `spawn_env_stamp` appends each pair after the boop-owned stamps, values through the existing `shell_quote` at :1148; `--dry-run` `cmd:` line shows them by construction. `run_fork` passes an empty vec.
- crates/boop/tests/: one test that `lane create --branch feature/x --brief <tmp> --preset flash4 --env A=1 --env "B=two words" --dry-run` prints `A='1'` and `B='two words'` on the `cmd:` line, and one that `--env NOEQUALS` exits non-zero with the clap error.
- issues/lane-create-env-flag/item.md: `status: fixed`, `## Tests Run`.

Do not touch any other file. Another lane owns bus.rs and supervise.rs today.

## Rules
1. `--env` is repeatable. A user pair named the same as a boop-owned stamp (`BOOP_SESSION`, `BOOP_LANE`, `BOOP_HARNESS`, `BOOP_PARENT`) is rejected with an error naming the key.
2. One commit, subject exactly: `boop: lane create --env KEY=VAL, repeatable, on the spawn env stamp and the dry-run line`.
3. Banned identifiers: provenance, substrate, load-bearing, regime.

## Validation (paste into the plans/ report)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/lane-env
cargo test -p boop 2>&1 | grep -E '^test result|FAILED'
cargo clippy -p boop --all-targets -- -D warnings 2>&1 | tail -3
cargo run -q -p boop -- beep lane create --branch feature/x --brief /tmp/x.md --preset flash4 --env A=1 --env 'B=two words' --dry-run 2>&1 | grep cmd:
git log --oneline -1
```
Known env-only failure, report and leave: `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung`.
