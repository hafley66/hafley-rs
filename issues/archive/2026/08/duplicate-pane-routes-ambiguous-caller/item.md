---
created: 2026-08-22
updated: 2026-08-25
type: bug
status: fixed
priority: normal
labels: [domain-boop, intent-fix]
size: S
closed: 2026-08-25
closed_by: claude-5
---

# One pane registered under two route names makes every `lane create` from it fail with "ambiguous caller"

## Receipt (2026-08-22 03:31)

`boop adopt` (SessionStart hook) registered pane `%2810` as `sprefa-coordinator`;
boop's own discovery had already registered the same pane as `claude-2810`.
`boop beep lane create ...` from that pane:
`Error: ambiguous caller: pane %2810 is registered as both claude-2810 and
sprefa-coordinator; prune one route`. Fixed by hand with
`boop beep lane delete claude-2810 --route-only`.

## Expected

`boop adopt` on a pane that already has an auto-discovered route replaces or
merges that route (the adopted name wins), so one pane has one route.

## Comments

### 2026-08-25T18:18:43Z · @claude-5

Superseded by env-only-identity and one-pane-register-path: adopt is gone, identity has no pane rung, so two routes on one pane cannot make a caller ambiguous; boop tui is the one register path.
