---
created: 2026-09-26
updated: 2026-09-27
type: bug
status: fixed
priority: high
labels: [domain-boop]
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
- [x] A successful steering response records a `steered` ladder row; a failed
      response records `steering-failed` with the error and keeps the hail for
      the next turn.
- [x] Other ACP channel adapters retain turn-boundary delivery.
- [x] A live hail to an active Codex lane records `steered` and its text appears
      in that lane's Codex transcript.

## Tests Run

The original fake-agent test accepted a string in `prompt`, so it did not catch
the wire-shape defect. The installed `@agentclientprotocol/codex-acp` parser
accepts `prompt: ContentBlock[]`; `injectSteerIntoActiveTurn` passes those blocks
to `codexAcpClient.steerTurn`. The request now sends
`[{"type":"text","text":"..."}]`.

## Resolution

### 2026-09-27T02:40:45Z · @codex

Reopened per m-a3721f4c: live hail m-14fb104c on supervisor 81211 recorded no
`steered` transition and its text was absent from `~/.codex/sessions`.

### 2026-09-27 · @codex

Fixed the ACP request prompt block shape and added `steered` / `steering-failed`
delivery transitions. Targeted channel, supervisor receipt, and store tests
pass. Live hail `m-cbf6330a` on supervisor 81211 recorded `appended` ->
`held-for-turn-boundary` -> `claimed-by-supervisor`, with no `steered` row; its
token was absent from Codex user/transcript content. The existing supervisor
predates this source change and must not be restarted, so keep the card open
until the coordinator installs the commit and a live hail proves both the row
and transcript text.

### 2026-09-27 · @codex

The coordinator's `m-cbf6330a` delivery reached this Codex context at the turn
boundary. Its token `BOOP_MIDTURN_PROOF_20260927_A7F3` is now present in this
turn's transcript, while the recorded ladder still has no `steered` transition.
This confirms next-turn delivery only; the live mid-turn acceptance criterion
remains open.

### 2026-09-27 · @codex

Live mid-turn proof: `m-df443652` reached the active turn before it ended. Its
ladder records `appended` -> `held-for-turn-boundary` -> `claimed-by-supervisor`
-> `steered` (`mid-turn steer accepted`) -> `submitted-to-harness` ->
`accepted-by-harness` (`midturn`). The text appears in
`~/.codex/sessions/2026/09/26/rollout-2026-09-26T20-39-17-01a0e04d-4dc0-72b0-9091-23519a5ecea6.jsonl:14628`.
