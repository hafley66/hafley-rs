---
created: 2026-08-17
updated: 2026-08-25
type: bug
status: fixed
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-correctness, component-supervisor]
size: S
closed: 2026-08-25
closed_by: claude-5
---

# 30-second first-signal limit kills healthy codex lanes

## Description

INCIDENT, 2026-08-17 22:24 local. Four codex lanes on `gpt-5.6-luna@medium`
died within 70 seconds of each other, across two independent coordinators, with
zero commits and clean worktrees. Every one printed the same three lines:

```
WARN  lane turn stalled; killing the harness child idle_ms=30469
INFO  lane turn ended turn_end_reason="stalled: 30s with no harness activity" turn_ok=false retryable=true
ERROR lane supervisor failed harness="codex" error=write rpc turn/start
```

| lane | brief bytes | died | commits |
|---|---|---|---|
| fix-boop-main-cli | 7595 | 22:24:53 | 0 |
| fix-boop-harness-model-spec | 6129 | 22:24:56 | 0 |
| fix-boop-spawn-guards | 4890 | 22:24:59 | 0 |
| feature-agent-network-frames (other coordinator, hafley-rxjs) | n/a | 22:24:36 | 0 |

## Root cause

`FIRST_SIGNAL_LIMIT` is a 30-second constant at
`crates/boop/src/supervise.rs:21`, applied at `:405-415` to the time between
the turn write and the FIRST harness event of that turn:

```rust
const FIRST_SIGNAL_LIMIT: Duration = Duration::from_secs(30);
```

`STALL_LIMIT` (`supervise.rs:24`) is 5 minutes and applies only AFTER the
harness has produced one event. A codex lane running reasoning at medium with
reasoning summaries off produces nothing on the wire until its first tool call,
so a large brief crosses 30 s before the first event and the supervisor kills a
healthy child.

The kill is not the whole failure. The retry writes the resume turn to the
already-killed child's stdin, which is the `write rpc turn/start` error at
`crates/boop/src/channel/codex.rs:81` through `channel/jsonrpc.rs`. So one slow
first token costs the lane, not one retry.

Ruled out, with receipts:

- Not a codex auth or model problem: `codex exec --model gpt-5.6-luna -c model_reasoning_effort=medium --skip-git-repo-check "reply with the single word OK"` returned `OK`, 9830 tokens, codex-cli 0.147.0, run at 22:26.
- Not a protocol break: the lane's own log shows `initialize`, `thread/start` and `conversation trace attached` all succeeding, with a real thread id (`01a011d3-69a0-7792-a8a4-80036172f321`), before the stall.
- Not brief size alone: the smallest brief (4890 bytes) died the same way.
- Not lane contention: the fourth lane belonged to a different coordinator in a different repo.

## Acceptance Criteria

- [ ] `FIRST_SIGNAL_LIMIT` is not a bare 30-second constant. It is either raised to a value a reasoning model can meet, or made per-harness, or fed by the harness declaring its own first-signal expectation.
- [ ] The retry after a stall kill does not write to a dead child: the resume opens a fresh channel, or the write error is caught and named rather than ending the supervisor.
- [ ] A stalled-then-killed lane reports WHY in its result body, so a coordinator reading `boop inbox drain` sees "stalled at first signal" and not a bare `rc=1`.
- [ ] Test: a fake channel that emits its first event after `FIRST_SIGNAL_LIMIT + 1` and then works normally must NOT kill the lane under the new rule. Fail-first receipt from today's tree.
- [ ] `docs/failure-modes.md` entry: incident, RCA, fail-pre-fix test, rail. This card does not close without one.
- [ ] Nothing seizes the machine: whatever the new limit is, a genuinely hung child is still killed, and that path keeps a test.

## Tests Run

## Implementation Notes

Reproduced by spawning any codex lane with a brief over roughly 5 KB at
`--preset luna`. The four supervise logs are at
`~/.agent/lanes/{fix-boop-main-cli,fix-boop-harness-model-spec,fix-boop-spawn-guards,feature-agent-network-frames}/supervise.log`.
`child.stderr` is zero bytes in every one of them, which is itself worth a
look: a killed child that wrote nothing to stderr gives a coordinator no
independent evidence.

This bug blocks every codex lane dispatch. It is why the 2026-08-17 boop audit
fix wave ran on native subagents instead.

## Comments

### 2026-08-25T18:18:43Z · @claude-5

Superseded: codex lanes launch through codex-acp only (codex-acp-only-launcher); DEFAULT_STALL_LIMIT is 30 minutes (supervise.rs:24), BOOP_STALL_LIMIT_SECS overrides; a stall ends the turn with turn_end_reason naming it and the parent gets the idle row with that reason (parent-visibility).
