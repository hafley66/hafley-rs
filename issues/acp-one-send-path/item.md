---
created: 2026-08-20
updated: 2026-09-27
type: improvement
status: fixed
priority: high
labels: [domain-boop, intent-design, needs-chris]
size: L
---

# ACP delivery for boop-owned lanes

## Description

## Description

Mail reaches a session by three different transports today, two of which type
into a keyboard or wait for a hook. `session/prompt` delivers a real user-role
turn and should be the only one.

| # | transport | who gets it | site |
|---|---|---|---|
| 1 | ACP `session/prompt` | a lane boop spawned, and only its own supervisor | `crates/boop-acp/src/channel/acp.rs:314` |
| 2 | tmux `load-buffer` + `paste-buffer` + `send-keys` | non-claude coordinators, any pane with no drain hook | `crates/boop/src/cli/mail.rs:201`, `crates/boop-mux/src/lib.rs:319` |
| 3 | hook inbox pull | claude coordinators, via `boop inbox drain` at a turn boundary | `crates/boop-proc/src/inbox.rs:55-108` |
| 4 | nothing | `kind=native` rows, `kind=result` rows | `crates/boop/src/cli/mail.rs:192-195` |

Measured 2026-08-20: a Fable lane running under Claude ACP called
`boop tell-parent`; the message reached a Codex coordinator by transport 2,
because `crates/boop/src/cli/me.rs:120` installs the drain hook only when
`harness == Some("claude")`, so `installed_for` was false and `deliver_hail`
fell through to the pane.

## The blocker is session ownership

boop can prompt over ACP only a session whose stdio it owns. `open_channel`
has three call sites, all inside lane supervision
(`crates/boop-proc/src/concatmap.rs:198,260`, `crates/boop/src/cli/job.rs:388`).
An adopted pane has a tmux target and no stdio pair, so no `AcpChannel` can be
built for it. `Route` carries `session_id` (`crates/boop/src/cli/me.rs:104`)
and nothing loads it into a channel outside `run_lane_supervisor`.

## Shape

Every session boop addresses is spawned by boop under an ACP adapter, including
coordinators. `adopt` stops writing a tmux target and starts owning a child.

| retires | replaced by |
|---|---|
| `boop adopt` + `write_inbox_hooks` | boop-owned ACP spawn |
| tmux paste injection, all four harness adapters | `session/prompt` |
| the "queued (no pane)" arm | a channel that always exists |
| `TuiChannel` (already unwired) | delete |

`session/resume` and `session/load` exist in the protocol so a coordinator can
survive a restart under this model.

## What ACP does NOT buy

Mid-turn delivery. `session/*` in `agent-client-protocol-schema-1.5.0` is
`new load resume fork list close delete prompt cancel set_mode
set_config_option request_permission update`. `prompt` is the only inbound
user-content method and it is one request per turn; `update` is agent to
client; `cancel` interrupts. boop already reflects this: `steer` returns
`Delivery::NextTurn` unconditionally
(`crates/boop-acp/src/channel/acp.rs:177-180`).

Anything that wants a message to land inside a running turn needs
`session/cancel` then `session/prompt`, which discards the turn in flight. That
is a separate decision and is NOT part of this card.

## Decision

2026-09-27: option (b). Scope this card to boop-owned lanes. Adopted panes keep
their inbox hooks and paste delivery. Mid-turn steering remains tracked by
`codex-midturn-mail`.

## Acceptance Criteria

- [x] Boop-owned lane sessions receive mail through their ACP channel.
- [x] Adopted panes retain their hook and pane delivery paths.
- [x] Mid-turn steering stays outside this card.

## Comments

### 2026-09-14T13:59:48Z · @codex

Cleanup 2026-09-14: resident ACP-host experiment retained at archive/boop-cleanup-20260914/feature/boop-acp-host (6821d85d79e9d442f9682c297fc3f67bf2df6a25). host.rs, agents.rs, ACP agent registry and CLI hosting are absent in main. Preserve for architecture review against current door-based delivery; no automatic merge or new test claim. Its idle worktree was removed.

### 2026-09-27 · @codex

Reproduced on installed `boop 0.0.10 (248dfdd3-dirty)`: one `boop beep` verb still reaches both ACP `session/prompt` (`crates/boop-proc/src/deliver.rs`) and tmux pane injection; `write_inbox_hooks` also remains in `crates/boop/src/cli/mail.rs`. Decision: should adopted coordinators become boop-owned ACP children so every send uses ACP, or should adopted-pane hooks/paste remain and this card narrow to boop-owned lanes?

### 2026-09-27 · @codex

Closed per decision (b): `run_dispatch` opens the harness ACP channel for a
boop-owned lane; adopted pane routes continue through their installed inbox
hook or pane fallback in `deliver_hail_budgeted`. The remaining mid-turn
question is tracked by `codex-midturn-mail`.
