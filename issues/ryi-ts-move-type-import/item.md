---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: move misses type-position import()

## Description

Defect D4 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:47:38Z · @feature-ryi-ts-refactor

Commits: 5a740949, 6acd97bd. Gate: UNVERIFIED, release build and crate tests terminated on the user's stop request; ryii, tsc, and dogfood cases unrun. Code and case scripts committed; coordinator runs gates serially. REPORT.md lists before/after behavior and the deferred invocation.
