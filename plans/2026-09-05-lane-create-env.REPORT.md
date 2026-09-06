# 2026-09-05 lane-create-env: REPORT

## Thing

Repeatable `boop beep lane create --env KEY=VAL` sets per-lane env vars on the
harness child, appended after boop's own identity stamps on the spawn command
and shown on the `--dry-run` `cmd:` line.

## Where it is now

Committed at `c0845c9` on `fix/lane-create-env`. Issue `lane-create-env-flag`
marked `done` with a Tests Run note.

## Next action

Review the commit; the pre-existing clippy failures below block a clean
`-D warnings` gate.

## Validation

```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/lane-env
cargo test -p boop 2>&1 | grep -E '^test result|FAILED'
```

```
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: FAILED. 97 passed; 4 failed (env/tmux flakiness, see deviations)
```

New test module `lane_create_env` passes 3/3:

- `env_pairs_ride_the_dry_run_cmd_line`
- `a_value_without_equals_is_refused_at_the_cli`
- `a_key_colliding_with_a_boop_stamp_is_refused_by_name`

```
cargo clippy -p boop --all-targets -- -D warnings 2>&1 | tail -3
```

```
error: could not compile `boop` (bin "boop") due to 2 previous errors
error: could not compile `boop` (bin "boop" test) due to 2 previous errors
```

Both clippy errors are pre-existing on the baseline (verified by stashing my
changes): `run_host` dead-code at `crates/boop/src/cli/debug.rs:186` and
`unnecessary-unwrap` at `crates/boop/src/cli/job.rs:1305`. Neither is in code
this lane owns or touches.

```
cargo run -q -p boop -- beep lane create --branch feature/x --brief /tmp/x.md --preset flash4 --env A=1 --env 'B=two words' --dry-run 2>&1 | grep cmd:
```

```
cmd: LC_ALL='en_US.UTF-8' LANG='en_US.UTF-8' BOOP_SESSION='feature-x' BOOP_LANE='feature-x' BOOP_HARNESS='opencode' BOOP_PARENT='fix-lane-create-env' A='1' B='two words' nice -n 10 boop beep lane run --lane 'feature-x' ...
```

The user pairs land after the boop-owned stamps and are shell-quoted.

```
git log --oneline -1
```

```
c0845c9 boop: lane create --env KEY=VAL, repeatable, on the spawn env stamp and the dry-run line
```

## Deviations from the brief

| Brief said | Reality | Disposition |
| --- | --- | --- |
| `item.md` status `fixed` with `## Tests Run` | status was `open`; no Tests Run section; schema rejects `fixed` for a feature | Set status to `done` (the feature completion state), appended Tests Run note |
| clippy `-D warnings` passes | pre-existing baseline failures at `debug.rs:186` and `job.rs:1305` | Left in place, out of lane scope; reported |
| one known test failure (`deliver_door`) | `deliver_door` plus `tell` failures flaky on baseline | `lane_carcass` failed in one run, passed in another; none use `--env` |

## Files changed

| File | Change |
| --- | --- |
| `crates/boop/src/main.rs` | `env: Vec<String>` on `LaneCmd::Create` with clap `value_parser` `parse_env_kv` rejecting no-`=` and empty-key |
| `crates/boop/src/cli/job.rs` | `LaneArgs.env`, `DispatchArgs.env`, `spawn_env_stamp` appends quoted pairs, `parse_env_pairs` rejects boop-stamp collisions, both construction sites |
| `crates/boop/tests/lane_create_env.rs` | new test module, registered in `tests/main.rs` |
