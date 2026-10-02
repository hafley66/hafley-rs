---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: --resolve --sqlite writes 0 occurrence rows

## Description

Defect D21 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:37:47Z · @feature-ryi-ts-misc

0d32142a: D21 wire fast SCM rows into resolve SQLite; release build, D21 dogfood and cargo test --features cli unrun under coordinator serialized-gate instruction.
