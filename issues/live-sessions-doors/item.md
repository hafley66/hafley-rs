---
created: 2026-08-22
updated: 2026-08-22
type: feature
status: open
priority: high
epic: harness-interface
related: ['@harness-id-capabilities']
labels: [domain-boop, intent-implementation]
size: M
---

# LiveSessions + Door traits, four impls

## Description

## Description

New traits `LiveSessions` and `Door` (plan §2) with four impls. Doors measured 2026-08-22 (research §1): claude `~/.claude/sessions/<pid>.json` + unix socket `{"type":"user","message":{…}}`; codex `state_5.sqlite` + `codex queue --remote`; opencode `GET /session`, `POST /session/:id/prompt_async`, SSE `/event`; kimi `Unreachable`. Lane P2a, sonnet high.

## Acceptance Criteria

- [ ] fixture test per door (temp sessions dir + socket echo; temp sqlite; HTTP stub; kimi unreachable)
- [ ] `agent_live` gains `door_kind`, `door_addr`; projection pass writes them
- [ ] no tmux capture or transcript mtime used for liveness
