---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: fixed
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
- [x] a `throw` edge kind lands in the cfg vocabulary
- [x] try-body throws and throwing calls reach the catch entry; rust/go/ts/kotlin each covered by a fixture
- [x] post-dominance computed from the cfg
- [x] CDG edges emitted under the existing `edge family=cfg` vocabulary, no new record kind
- [x] `extract graph --slice PATH:BYTE` returns a closed statement set on one fixture
- [x] `cargo test --features cli,read --test all -j 2` green

G1 repro receipt: `ryii --kinds cfg /tmp/cdg-throw-repro.ts` showed no incoming edge to `catch_clause` and sent the try-body throw to callable exit. The G1 fixture now asserts call-site and explicit throw edges into the catch entry; a throw from the catch body still reaches callable exit.

G1 verification receipts:
- `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run --features cli -j 2 --offline --locked --test all` from `crates/sprefa-extract`: 1124 passed, 18 skipped.
- `scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/release` from `crates/sprefa-extract`: 14/14 passed.
- `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run --workspace -j 2 -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'`: 1378 passed, 203 skipped, 1 leaky.

## Tests Run

- Red: before control-dependence generation, the new `ts_if_emits_control_dependence_in_the_cfg_family` assertion has no `control` edges for either arm. Green: it now pins the branch-to-arm edges, and `control_slice_returns_a_closed_statement_set` pins the backwards closure for `allow()` without including `after()`.
- `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run --features cli,read --locked -j 2 --test all -E 'test(/t_17_cfg_first_plane::|t_124_cfg_python_prolog::|control_slice_returns_a_closed_statement_set/)'`: 17 passed.
- `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo test --features cli,read --test all -j 2`: 1,130 passed, 0 failed, 18 ignored.
- Review follow-up: replaced the hand-written post-dominator fixpoint with `petgraph::algo::dominators::simple_fast` over the reversed per-callable CFG. `cargo check -p hafley_scm --offline -j 2` passes; the same focused CFG/slice selection passes 17/17 with `petgraph` 0.8.3 in both lockfiles.

## Implementation Notes

Land after `extract-lines-flag` and `extract-graph-verb`. Nothing in those two depends on this.

## Decisions

Should G1 throw-to-catch edges land as a separate increment before G2 post-dominance, CDG, and slicing?
