---
created: 2026-09-02
updated: 2026-09-02
type: bug
reporter: hafley66@gmail.com
status: open
priority: normal
---

# boop: transport readiness probe collides with running one-shot opencode lanes

## Description

Observed (2026-09-02, ascii-renderer perf-instrumentation lanes, flash4 preset = openrouter/deepseek/deepseek-v4-flash-0731 via opencode, deepinfra pin):

- 5 of 6 `boop lane create` spawns died with rc=1 or rc=4 leaving zero or partial files in the tree.
- Signature: a transport readiness probe message ("respond exactly with: boop") lands in the lane mid-run, while the model is executing a multi-file brief. The model answers "boop", opencode treats the turn as complete, the one-shot run exits, remaining brief steps never run.
- Hailing a parked lane with "restart the brief" recovered two lanes partially; the coordinator finished the rest in-tree.
- Only one lane (perf-instr-modes-dir) completed on its own; merged as ascii-renderer commit 4aa6feb.

Probe source: `crates/boop-proc/src/supervise.rs:28-31` (`START_ACK_PROMPT`), gated by `start_ack_pending` at lines 690, 726, 746-747, 811-831, 879-980. The probe is coded to run as a distinct first turn before the brief is submitted (`start_ack_pending = lane.resume.is_none()` at line 690, brief only sent after `start_ack_pending = false` at line 979), but the observed failures show the probe reaching the model while a brief turn is already in flight.

Expected:

- The probe never reaches a lane that has a prompt in flight, or it is sent on a side channel the model cannot see, or its reply is consumed by boop without ending the model turn.

Suggestions to evaluate (not decisions):

- Gate the probe on lane state: skip when a turn is active.
- Probe once at spawn before the brief is delivered, then rely on ACP session liveness instead of a model round-trip.
- Mark the probe reply as out-of-band so a one-shot run continues to the brief's end.

Note: lane briefs currently must carry CARGO_TARGET_DIR per lane because `boop lane create` has no env flag. Separate small feature request, filed as a second issue if the tracker supports it cheaply: `boop lane create: --env KEY=VAL flag`.
