---
created: 2026-09-26
updated: 2026-09-26
type: bug
status: fixed
priority: high
labels: [domain-boop]
closed: 2026-09-26
closed_by: codex
---

# Codex lane mail waits for the active ACP turn to end

## Reproduction

On the installed boop 0.0.10 (0080597a), the stored row `m-00b3221d` is a
request from `claude-375` to `chore-the-gang-edits`. A read-only query against
`~/.agent/boop.db` shows transitions `appended`, `held-for-turn-boundary
(lane supervisor)`, and `claimed-by-supervisor (inbox drain)`; `to_timestamp`
is NULL. The reported Codex lane held this hail for over an hour while one ACP
turn remained active.

The ACP lane channel returns `Delivery::NextTurn` from `steer`, and the
supervisor retains the hail until the active turn ends. Codex ACP exposes its
`_session/steering` extension, which queues text into the current Codex turn.

## Expected

A hail to a Codex ACP lane is admitted to Codex's mid-turn queue while the
current turn is running, and the delivery receipt records that queue admission.
Other ACP lane adapters retain their existing delivery behavior.

## Acceptance Criteria

- [x] A Codex ACP channel sends a hail through `_session/steering` during an
      active `session/prompt` request.
- [x] A successful `injected` or `startedNewTurn` response records accepted
      delivery; a `failed` response remains queued for the next turn.
- [x] Other ACP channel adapters retain turn-boundary delivery.

## Tests Run

`cargo nextest run -p boop-acp -j 2 -E 'test(codex_steering_extension_admits_mail_during_an_active_prompt) | test(every_roster_row_spawns_something_in_acp_mode) | test(no_text_reaches_a_turn_already_in_flight)'` -> 3 passed.

## Resolution

### 2026-09-27T02:40:45Z · @codex

Repro m-00b3221d held at the turn boundary; Codex ACP _session/steering now admits active-turn mail; cargo nextest run -p boop-acp -j 2 -E targeted steering and roster tests -> 3 passed.
