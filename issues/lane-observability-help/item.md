---
created: 2026-08-13
updated: 2026-08-14
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-documentation, intent-doctrine]
lane: boop-docs
lane_seq: 8
collision: [boop-help]
blocked_by: ['@lane-status-command']
---

# 008 Document lane observability doctrine

## Description

## Objective

Add the canonical monitoring workflow to `boop --help`.

## Acceptance Criteria

- [ ] Help directs users to lane status, usage, and tail commands.
- [ ] Help forbids normal monitoring through raw agent_trace_span, dict_session, agent_usage, and agent_turn queries.
- [ ] Help defines the silent-death signature.
- [ ] Examples cover active progress, pre-turn failure, and clean completion.
- [ ] Existing spawn, completion, liveness, and trace doctrine remains consistent.

## Decisions

### 2026-08-14T03:37:02Z · @codex

Consolidated scope: document the public lane and beep command families and own deterministic end-to-end fixtures for pre-turn death, active token movement, tool work, clean completion, silent death, OpenCode generated sessions, and Codex replacement sessions.

## Comments

### 2026-08-15T00:19:47Z · @codex

Observed 2026-08-14: top-level boop --help documents , while  accepts  and rejects . Align the doctrine and subcommand contract.

### 2026-08-15T00:20:07Z · @codex

Correction: top-level boop help documents --wait-timeout SECONDS, while boop beep lane wait accepts --timeout SECONDS and rejects --wait-timeout. Align the doctrine and subcommand contract.

