---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: --slow without ts-checker exits 0 with 0 edges

## Description

Defect D15 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:51:41Z · @feature-ryi-ts-slow

71669f2f + a1d1da29: D15 missing-feature preflight, CLI regression, and D15.sh committed. Gate UNRUN by user instruction; coordinator needs cli-only RYII_NO_TS_CHECKER and serialized release/dogfood/crate gates.
