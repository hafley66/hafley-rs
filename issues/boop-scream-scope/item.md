---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: high
epic: burndown-2026-10
related: ['@boop-identity-from-pid']
labels: [boop]
---

# boop scream/shout hit every agent, not the caller's own tree

## Description

2026-10-02T14:31Z coordinator claude-375 ran 'boop beep scream <body>' to reach its 6 ryi lanes; it also sent Esc + body to unrelated sessions claude-44 and claude-471 (agent_mail hail rows). Required: scream/shout default scope = the caller's descendants (lanes it spawned + their children, from parent edges and the pid/agent graph); 'everyone' becomes an explicit opt-in. Same pid walk as boop-identity-from-pid.
