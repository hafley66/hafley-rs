---
created: 2026-10-01
updated: 2026-10-01
type: task
status: open
priority: normal
epic: burndown-2026-10
labels: [boop]
---

# Migrate user slice: typed favorite source refs, dict fix for its tables

## Description

agent_favorite.source is a string (turn: 261, codex: 13, session: 2, agent_session: 1, turn-range: 1, empty 23; 1 orphan). Make typed refs; dict_mood_name -> enum column. Rehearse on a backup copy (~/backups/boop).
