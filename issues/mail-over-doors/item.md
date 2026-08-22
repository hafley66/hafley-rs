---
created: 2026-08-22
updated: 2026-08-22
type: task
status: open
priority: high
epic: harness-interface
related: ['@live-sessions-doors', '@acp-one-send-path']
labels: [domain-boop, intent-implementation]
size: M
---

# deliver_hail through Harness::door, agent_delivery rows

## Description

## Description

`deliver_hail` (`boop/src/cli/mail.rs:161-202`, five early returns, tmux `send-keys` arm) becomes one call through `Harness::door()` per `Capabilities.mail`; outcome recorded in new `agent_delivery(message_id, route, harness_id, outcome, at_ms)`; `boop wait` reads it. Supersedes `acp-one-send-path` transport rows 2 and 3. Lane P2b, sonnet high.

## Acceptance Criteria

- [ ] keystroke delivery removed; claude hook inbox path kept behind `MailPolicy::TurnBoundaryHook` until card 4
- [ ] live receipt: `boop beep hail` lands in a real `boop tui claude` pane and a `boop tui codex` pane
- [ ] `agent_delivery` PK `(message_id, route)`
