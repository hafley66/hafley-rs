---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: dismantle is a TODO

## Description

Defect D23 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:37:47Z · @feature-ryi-ts-misc

311f671b: D23 removed TODO command from schema, CLI and transports; release build, D23 dogfood, generator parity and cargo test --features cli unrun under coordinator serialized-gate instruction.

### 2026-10-02T14:38:16Z · @feature-ryi-ts-misc

D23 follow-up: dogfood parser assertion accepts root PATH parsing rejection as well as unknown-subcommand rejection; no runtime commands or gates executed. See latest D23 commit.

