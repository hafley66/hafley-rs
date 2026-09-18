---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# cfg: throw edges, post-dominance and the CDG edge color

## Description

## Description

Split out of `extract-graph-verb` so a research increment does not hold three days of usable work hostage.

Two gaps, both in the cfg plane.

### G1 exception edges

Verified 2026-09-18 on `~/projects/instant/src/0_boopSelection.ts`, function `parseJson`, bytes [2089,2419), every span checked against source text:

```
entry  [2089,2419)
  next   -> stmt   [2137,2164)   const text = stdout.trim()
  next   -> branch [2167,2236)   if (!text)
    arm  -> ret    [2178,2236)   throw Error("empty output")   -> exit
    next -> ret    [2249,2273)   return JSON.parse(text)       -> exit
         stmt   [2285,2286)   catch param e     <- ZERO incoming edges
  next   -> ret    [2294,2413)   throw Error("invalid JSON")   -> exit
exit   [2089,2419)
```

`stmt [2285,2286)` is the catch clause. Nothing connects the try body to it. Any slice crossing a throw is wrong. Needs a `throw` edge kind from every call site and throw statement inside a try block to the catch entry, in rust, go, ts and kotlin.

### G2 post-dominance and the CDG edge color

`extract --schema:210` states it: "Post-dominance and the CDG edge color are NOT built." Program slicing cannot exist without it.

`crates/sprefa-extract/AGENTS.md:51` already licenses this as a facet: "control dependence (dominance from cst) -> program slicing". No amendment needed.

`AGENTS.md:52` names taint and slicing the two highest-value next programs, and both ride control dependence. This is the prerequisite for both.

## Acceptance Criteria
- [ ] a `throw` edge kind lands in the cfg vocabulary
- [ ] `parseJson`'s catch clause has an incoming edge, rust/go/ts/kotlin each covered by a fixture
- [ ] post-dominance computed from the cfg
- [ ] CDG edges emitted under the existing `edge family=cfg` vocabulary, no new record kind
- [ ] `extract graph --slice PATH:BYTE` returns a closed statement set on one fixture
- [ ] `cargo test --features cli` green

## Tests Run

## Implementation Notes

Land after `extract-lines-flag` and `extract-graph-verb`. Nothing in those two depends on this.
