---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: callers emit closure sites twice

## Description

Defect D13 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:45:56Z · @feature-ryi-ts-graph

f722b9ae, 002abf6c; D13 existing callers query selects one row per full call-site/target span; raw mirror facts retained; D13.sh written. Gate: unverified; build/tests/dogfood deferred by coordinator instruction.
