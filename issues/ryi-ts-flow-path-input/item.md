---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: flow-path input format undocumented, 0 paths

## Description

Defect D25 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:37:47Z · @feature-ryi-ts-misc

1e4bfcd7: D25 PATH normalization and tagged-digest help; partial: nonempty children paths still require local df traversal, excluded by later no-analysis/no-walker scope. Case preserves nonempty assertion. Release build, D25 dogfood and cargo test --features cli unrun under coordinator serialized-gate instruction.
