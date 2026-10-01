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

# Migrate user slice: typed favorite source refs, dict fix for its tables

## Description

agent_favorite.source is a string (turn: 261, codex: 13, session: 2, agent_session: 1, turn-range: 1, empty 23; 1 orphan). Make typed refs; dict_mood_name -> enum column. Rehearse on a backup copy (~/backups/boop).

## Comments

### 2026-10-01T23:17:08Z · @claude-375

hafley-rs main c5dfc29f, boop2 main f56c441. Schema 39: FavoriteSourceKind typed columns + original text kept; mood enum. Rehearsal on backup copy: 303/303 bodies identical, counts equal. Suite 0 not-ok on main build. Live db untouched (v38) until the user installs.
