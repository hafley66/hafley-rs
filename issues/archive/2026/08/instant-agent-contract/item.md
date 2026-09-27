---
created: 2026-08-14
updated: 2026-08-14
type: task
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, domain-instant, intent-design, artifact-plan]
lane: boop-contract
lane_seq: 9
collision: [instant-agent-contract]
related: ['@lane-runtime-identity', '@lane-status-command']
assignee: luna
closed: 2026-08-14
---

# 009 Map Instant agent consumer contract

## Description

## Objective

Freeze the Instant consumer contract before changing Boop or Instant. Map current tmux, shell, harness-ledger, CASS, mailbox, and Boop inputs to the fields rendered by the Agents and Harness Trace panels.

## Deliverable

`crates/boop/plans/2026-08-15-instant-agent-projection.md` containing exact source file and line references, type signatures, row cardinality, joins, status formulas, and ownership of acquisition, derivation, and presentation.

## Acceptance Criteria

- [x] Catalogs every Instant Rust and TypeScript reader used by the Agents, CASS, and Harness Trace panels.
- [x] Records message-count formulas by user, assistant, tool call, session, trace, and lane.
- [x] Records tmux session, pane, PID, shell-only lane, route, mailbox, worktree, and completion joins.
- [x] Marks each field as stored fact, read-time derivation, or Instant presentation state.
- [x] Maps each field to an existing Boop table/query or a named gap.
- [x] Separates CASS issue/reservation data from agent transcript and runtime data.
- [x] Names deterministic fixtures that pin Claude, Codex, OpenCode, Kimi, and shell-only behavior.

## Tests Run

- [x] `issuectl doctor`
- [x] Source references verified with `rg`

## Implementation Notes

Read-only design artifact. The current Instant checkout contains uncommitted work, so the audit reads that checkout directly and does not modify it.

## Agent Runs

### 2026-08-15T00:19:53Z · @codex

Two flash4 read-only audits were dispatched through Boop and exited rc=1 before transcript attachment. No stale REPORT.md content was accepted. Contract work remains head-of-line and can proceed from direct source inspection.

### 2026-08-15T00:21:34Z · @codex

Dispatching flash4 through Boop lane chore-instant-agent-contract-map. The lane must verify current Instant and Boop source directly and write the contract plan only.

### 2026-08-15T00:23:29Z · @codex

Boop lane chore-instant-agent-contract-map exited rc=1 before attaching a model session and retained a stale report. Re-dispatched as native Luna read-only audit instant_agent_contract.

## Comments

### 2026-08-15T00:53:46Z · @codex

Native Luna audit completed against current Instant and Boop sources. The durable boundary is recorded in crates/boop/plans/2026-08-15-instant-agent-projection.md. Instant remains unchanged.
