---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: callers ignore FILE#name and Class.method anchors

## Description

Defect D16 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:45:56Z · @feature-ryi-ts-graph

bfae2b92, 002abf6c; D16 FILE#name filters existing callers rows, Class.method returns explicit usage error as allowed by plan; D16.sh written. Gate: unverified; build/tests/dogfood deferred by coordinator instruction.
