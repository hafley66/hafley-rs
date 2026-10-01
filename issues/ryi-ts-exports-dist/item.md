---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: high
epic: burndown-2026-10
labels: [ts]
---

# ryi TS: rename silently misses consumers importing via package exports -> dist

## Description

342 of 486 @hafley66/* import pairs never resolve in hafley-rxjs; only tsconfig paths and exports->src resolve. rename exits 0, no abstain (toSignal 2/22 files, GraphId 7/31). Report: plans/2026-10-01-ryi-ts-utility.md D1-D2; D3-D25 listed there.
