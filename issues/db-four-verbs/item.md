---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
---

# db is sql, chat, status, sync

## Description

## Description

`db` still exposes 7 verbs after the audit hid 9; every table dump is `boop db "<sql>"`.

Cut: `db <sql>`, `db chat`, `db status`, `db sync` visible; `usage`, `price`, `favorite`, `sync-cursor` hidden.

## Acceptance Criteria

- [ ] `boop db --help` lists 4
