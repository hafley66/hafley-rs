---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: fast skips .d.ts files

## Description

Defect D24 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:38:53Z · @feature-ryi-ts-exports

D24 214f24b7. Gate UNRUN: user/coordinator stopped parallel builds and tests. D24.sh written for fast JSONL and SQLite declaration file facts; release build, dogfood, and cargo test --features cli deferred for serialized execution. Details in REPORT.md.
