---
created: 2026-10-01
updated: 2026-10-01
type: task
status: open
priority: normal
epic: burndown-2026-10
labels: [boop2]
---

# boop tsp: wire models adopt schema-40 enums (remove 16 key-type suppressions)

## Description

Item 5 added 16 #suppress boop/key-type: table columns are enums at schema 40 while wire models keep string for the same keys (13 enum domains, 3 open text). Make wire models use the enums where the producer is closed (see plans/dict-closed-sets/18_check_audit.tsv); checker exempt_suppressed back to 9.
