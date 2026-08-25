---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: M
---

# Claude delivery through the door only; hook inbox is a rung, not a verb group

## Description

## Description

Claude coordinators take mail by unix-socket door or by `.claude/settings.json` hooks (`inbox drain`, `inbox hooks`). Two paths, one verb group that exists for the second.

Cut: hook inbox becomes an internal ladder rung; `inbox` verbs hidden.

## Acceptance Criteria

- [ ] `boop --help` has no `inbox`
- [ ] claude coordinator e2e receives a row with no hooks installed
