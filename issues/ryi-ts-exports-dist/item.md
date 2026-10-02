---
created: 2026-10-01
updated: 2026-10-02
type: bug
status: open
priority: high
epic: burndown-2026-10
labels: [ts]
---

# ryi TS: rename silently misses consumers importing via package exports -> dist

## Description

342 of 486 @hafley66/* import pairs never resolve in hafley-rxjs; only tsconfig paths and exports->src resolve. rename exits 0, no abstain (toSignal 2/22 files, GraphId 7/31). Report: plans/2026-10-01-ryi-ts-utility.md D1-D2; D3-D25 listed there.

## Comments

### 2026-10-02T14:38:53Z · @feature-ryi-ts-exports

D2 797d4d62; D1 a96d7b6e. Gate UNRUN: user/coordinator stopped parallel builds and tests. D02.sh and D01.sh written, corpus rxjs-corpus-feature-ryi-ts-exports at d0802620. Required release build, D01/D02 dogfood, and cargo test --features cli deferred for serialized execution; no measured after counts or tsc claim. Details in REPORT.md.
