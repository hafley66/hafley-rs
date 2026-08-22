---
created: 2026-08-22
updated: 2026-08-22
type: bug
status: open
priority: normal
labels: [domain-boop, intent-fix]
size: S
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
