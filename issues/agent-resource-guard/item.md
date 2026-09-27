---
created: 2026-09-17
updated: 2026-09-27
type: improvement
status: in-progress
priority: normal
labels: [domain-boop, deferred]
---

# Deferred: per-agent process history and memory guard

## Description

## Status and scope
Revived on 2026-09-27. Implement the opted-in process and RSS guard and its pause/resume seam.

## Requested behavior
Opt-in Boop config for process-tree sampling interval and tree RSS ceiling N. Trace full poll duration, process count and attribution cost. Reuse sysinfo and existing ps/pstree; record PID plus start time, parent relationships and bounded sample/trigger receipts without credentials or raw environment dumps.

## Breach policy
Sustained sampled breach invokes the existing harness/channel interrupt/cancel operation once, waits a configurable grace period while observing usage, then pauses the verified owned agent tree if still over limit. Explicit manual resume. Exclude monitor, tmux server and shared daemons. Ownership and process incarnation must be revalidated. Cancellation acknowledgement and cancellation failure must be observable.

## Limits
Polling permits overshoot and misses short-lived descendants. Summed RSS double-counts some shared memory. Pausing retains allocated RAM. Kernel hard caps and complete spawn events require separate OS support. ACP-owned LaneChannel interrupt and attached-TUI Door cancellation have different ownership requirements.

## Implementation receipt
The opt-in RSS guard samples only the harness child process group, revalidates its leader PID and start time before samples and signals, requests channel cancellation after sustained over-limit samples, and pauses the group after the grace interval if RSS remains over limit. Trace rows record sampling cost, process count, interrupt queue result, and pause/resume outcomes. `boop beep lane resume <lane>` resumes a resource-guard pause.

The ACP adapter reports the group leader started by `agent-client-protocol` with `process_group(0)`; the Claude child also starts with `process_group(0)`. The supervisor remains in its parent group. Cancellation reports whether the request was queued to the channel; provider-side cancellation acknowledgement is not available through `LaneChannel::interrupt`.

Tests: `resource_guard::tests::pauses_and_resumes_a_child_in_its_own_process_group` spawns `sleep 30`, verifies `ps` state `T`, resumes, verifies running state, and kills the child. State tests cover sustained breach, grace, and recovery. `terminal_wire_tests::spawned_agent_reports_its_owned_process_group` verifies the ACP subprocess group receipt.

## Related priority
[Instant turn-square tracker](../../../instant/issues/tui-renderer-testing/item.md). Existing substrate: boop-store/src/proc.rs and boop-acp LaneChannel::interrupt.

## Decisions

### 2026-09-27T04:06:56Z · @codex

Revived by the 2026-09-27 request to process all non-graph agent-* cards. Current installed boop --help exposes no resource-guard option, and the current boop-* source has no resource-guard implementation.

### 2026-09-27T04:10:23Z · @codex

Current repro: installed boop --help has no guard option and the current boop source has no guard monitor. The issue specifies cancellation followed by pausing the verified owned process tree, but the supervisor runs in that tree and no process-suspend API exists. Resolve the process ownership boundary before implementing pause/resume.

### 2026-09-27 · user decision

Suspend API: the supervisor stays outside the harness child process group. Start the harness child with `process_group(0)` using `std::os::unix::process::CommandExt`; pause and resume the group through `nix::sys::signal::killpg(pgid, SIGSTOP/SIGCONT)`. Verify with a `sleep 30` child spawned through the same path, `ps -o stat`, stop/continue assertions, then kill it.
