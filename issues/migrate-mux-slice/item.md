---
created: 2026-10-01
updated: 2026-10-01
type: task
status: done
priority: normal
epic: burndown-2026-10
labels: [boop]
closed: 2026-10-01
---

# Migrate mux slice: tmux only in boop-mux, tmux argv contract, ControlEvent

## Description

5 Command::new("tmux") outside boop-mux (boop 2, boop-proc 3). boop tsp mux/: add ControlEvent/Notification unions, ControlClient, tmux argv ops. Suite green after.

## Comments

### 2026-10-01T22:45:27Z · @claude-375

hafley-rs main f9483da5, boop2 main ffa2b51. tmux spawns outside boop-mux: 0 (ryii). cargo boop-mux 59, boop-proc 183, boop 170 passed. Suite 0 fail; real db 306|418 unchanged. Key checker 0.
