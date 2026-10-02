---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi, ts]
---

# ryi TS: callers bind DOM globals to corpus names (phantom stratify cycle)

## Description

Defect D10,D11 in plans/2026-10-01-ryi-ts-utility.md (repro command, expected, observed). Corpus hafley-rxjs @ d0802620.

## Comments

### 2026-10-02T14:45:56Z · @feature-ryi-ts-graph

7ed47bee; D10/D11 lexical/import binding guard, no corpus-unique fallback, D10.sh and D11.sh written. Gate: unverified; build/tests/dogfood deferred by coordinator instruction.

### 2026-10-02T14:46:57Z · @feature-ryi-ts-graph

Supplement: constructor lexical coverage in 3f145c58 if present; see latest D10 commit for exact hash. Gates remain unverified.

### 2026-10-02T14:47:12Z · @feature-ryi-ts-graph

Correction to supplemental note: constructor lexical coverage commit is d2922cf5. The preceding hash was erroneous. Gates remain unverified.


