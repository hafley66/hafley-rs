---
created: 2026-08-22
updated: 2026-08-22
type: feature
status: done
priority: high
epic: harness-interface
related: ['@harness-id-capabilities']
labels: [domain-boop, intent-implementation]
size: M
closed: 2026-08-22
---

# LiveSessions + Door traits, four impls

## Description

## Description

New traits `LiveSessions` and `Door` (plan §2) with four impls. Doors measured 2026-08-22 (research §1): claude `~/.claude/sessions/<pid>.json` + unix socket `{"type":"user","message":{…}}`; codex `state_5.sqlite` + `codex queue --remote`; opencode `GET /session`, `POST /session/:id/prompt_async`, SSE `/event`; kimi `Unreachable`. Lane P2a, sonnet high.

## Acceptance Criteria

- [ ] fixture test per door (temp sessions dir + socket echo; temp sqlite; HTTP stub; kimi unreachable)
- [ ] `agent_live` gains `door_kind`, `door_addr`; projection pass writes them
- [ ] no tmux capture or transcript mtime used for liveness

## Comments

### 2026-08-22T22:45:06Z · @fable

Landed in PR #47: 275f910, e37c523. Four LiveSessions + Door impls; claude auth line verified against the 2.1.240 binary; opencode routes from /doc; codex notify_idle still Err (app-server stream unread).
