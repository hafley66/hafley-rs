# Brief: a lane that ends its turn is idle, never dead

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop
and report. Never spawn subagents. Commit every green step. PR against `main`.
`timeout` on every command; `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`.

## The user's decision (2026-08-21, verbatim)
"that is opposite behavior i want out of boop." Context: two sprefa lanes ended a turn to
report a finding / wait on a background job; boop closed their channel, the pane exited,
`lane list` read `dead`, and their reports were lost in the pane.

## Today, the sites
- `crates/boop-proc/src/supervise.rs:751-773`: after a turn ends, if `pending(...)` holds no
  hail, the supervisor calls `channel.close()`, computes `completion_verdict` and returns
  `Ended`; `lane run` exits and the tmux session with it.
- `supervise.rs:796 completion_verdict`: a turn that ends without the brief-done marker is
  `rc=1 "agent stopped before completing the brief"`.
- `supervise.rs:21 STALL_LIMIT = 5 min` and `:31 stalled`: an idle turn is killed and
  resumed ("turn stalled (300s idle), retrying"), which kills a lane waiting on a
  background build.
- `crates/boop/src/cli/job.rs:1450 lane_state`: live iff the tmux target is alive.
- `job.rs:2470 impl Drop for LiveTmuxSession` kills the session on drop.

## Build exactly this
1. After a turn ends with no pending hail the lane stays RESIDENT: channel open, process
   alive, state `idle`. The supervisor parks on the mailbox (`pending`) and on a stop
   request; the next hail is the next turn (the `held` branch already does this, make it
   the only path). `Ended` is returned only on: an explicit `lane delete`, the harness
   process dying, or the brief-done marker (then the lane still stays resident until
   deleted; the result row is written at the marker, not at exit).
2. `lane list` grows a third state: `live` (turn running), `idle` (resident, between
   turns), `dead` (process or pane gone). `lane_state` reads the supervisor's state file,
   not only the tmux target.
3. The stall rule applies to a RUNNING turn only and only when the harness reports no tool
   activity; a turn whose last tool call is a Bash with `run_in_background` or an
   `until`-loop is not stalled. Raise `STALL_LIMIT` to 30 min and make it a config key.
4. `completion_verdict` stops inventing rc=1 for "stopped before completing": an idle lane
   has no verdict yet.
5. A hail to an idle lane wakes it within one `POLL` (700ms); a hail to a lane mid-turn is
   held until the turn ends (existing behaviour).

## Receipts
- FAIL-FIRST tests in `supervise.rs` (the file already has a battery at `:1100+`): a turn
  ending with no pending hail leaves the supervisor parked and the channel open; a hail then
  starts turn 2 on the same conversation id; `lane delete` ends it. Each red before the fix,
  paste the red run.
- `lane list` shows `idle` for a parked lane (integration test through the tmux fake in
  `cli/me.rs:337`).
- A stalled-turn test: background Bash activity is not stalled at 5 min.
- `cargo test -p boop -p boop-proc -p boop-store` green plus yours; `cargo clippy` clean.
- Failure ledger entry in the repo's ledger (grep `failure-modes`); docs: `crates/boop/docs/`
  gains `lane-lifecycle.md` with a stateDiagram of live/idle/dead and the three exits.

## Ownership
Yours: `crates/boop-proc/src/supervise.rs`, `crates/boop/src/cli/job.rs` (lane_state, list,
delete), `crates/boop-store/src/trail.rs` (DeadReason gains nothing; an idle lane has no
dead reason), config key plumbing, docs. FORBIDDEN: the ACP channel crate internals
(`crates/boop-acp`) beyond calling `close()` later, `crates/soopy/**`.

## Reaching the coordinator
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<one line>"`. Use it when
blocked, when done (PR number + gate numbers), when this brief is wrong.

## Turn law (until your own fix lands)
Never end your turn before the PR is posted and its gate numbers are hailed. Poll
background jobs with an `until` loop inside one Bash call.

## Style laws
No em dashes. Banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only. Comment budget: constraints only.
