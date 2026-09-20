---
created: 2026-09-17
updated: 2026-09-17
type: improvement
status: open
priority: normal
labels: [domain-boop, deferred]
---

# Deferred: per-agent process history and memory guard

## Description

## Status and scope
Deferred at user request on 2026-09-17. Turn-square correctness in Instant takes priority. Do not enable or continue resource guard work until requested.

## Requested behavior
Opt-in Boop config for process-tree sampling interval and tree RSS ceiling N. Trace full poll duration, process count and attribution cost. Reuse sysinfo and existing ps/pstree; record PID plus start time, parent relationships and bounded sample/trigger receipts without credentials or raw environment dumps.

## Breach policy
Sustained sampled breach invokes the existing harness/channel interrupt/cancel operation once, waits a configurable grace period while observing usage, then pauses the verified owned agent tree if still over limit. Explicit manual resume. Exclude monitor, tmux server and shared daemons. Ownership and process incarnation must be revalidated. Cancellation acknowledgement and cancellation failure must be observable.

## Limits
Polling permits overshoot and misses short-lived descendants. Summed RSS double-counts some shared memory. Pausing retains allocated RAM. Kernel hard caps and complete spawn events require separate OS support. ACP-owned LaneChannel interrupt and attached-TUI Door cancellation have different ownership requirements.

## Existing partial work
Luna has left opt-in config, sampler refresh, guard state/tests, poll traces and LaneChannel interrupt integration in the working tree. No host configuration was enabled. Agent reported pause as an unsupported seam, so combined cancellation/pause acceptance is incomplete. Review the diff and cancellation result propagation before resuming; do not treat it as a completed enforcement mechanism.

## Acceptance for later
Deterministic fake-tree/clock tests for identity reuse, sustained breach, cancellation, grace, pause/resume and recovery. Bounded isolated integration test without touching live user agents. Targeted repository gates only while concurrent lanes exist.

## Related priority
[Instant turn-square tracker](../../../instant/issues/tui-renderer-testing/item.md). Existing substrate: boop-store/src/proc.rs and boop-acp LaneChannel::interrupt.
