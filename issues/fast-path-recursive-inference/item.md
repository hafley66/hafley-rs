---
created: 2026-09-19
updated: 2026-09-26
type: feature
status: open
priority: normal
epic: ryi-fast-tier
related: ['@local-binding-inference', '@extract-graph-verb', '@kind-vocab-constraint']
labels: [extract, intent-architecture]
---

# fast path infers receiver types by recursive fixpoint over its own facts

## Description

## Description

User-set 2026-09-19: the fast (diet) path should make PARTIAL attempts at
inferring its own facts through recursive datalog over what it already emits,
and the measurement of success is that the baseline counts below change.

### The baseline, measured 2026-09-19

`ryi --family call` over all 101 `.rs` files of `crates/sprefa-extract/src`,
24873 `site` rows, binary built from `9b25f783`:

| callee name | sites |
| --- | --- |
| `contains` | 195 |
| `find` | 190 |
| `ends_with` | 58 |
| `starts_with` | 57 |
| `strip_prefix` | 52 |
| `strip_suffix` | 18 |
| `matches` | 3 |
| `rfind` | 3 |
| `to_lowercase` | 2 |

Every one of those 195 `contains` rows is the same row shape whether the
receiver is a `str`, a `Vec`, a `HashSet` or a `BTreeSet`. The fast path
name-matches, so it cannot separate them. Asking "where does this binary do a
string filter" is unanswerable from the fast path today.

`--family scip` answers it, because rust-analyzer gives `str::contains` and
`Vec::contains` distinct SCIP symbols. That costs an indexer run, a toolchain,
and a budget. The goal here is to recover SOME of that answer without either.

### The shape wanted

Facts the fast path already emits (`resolved_edge`, `unresolved`, the type
plane's `TypeEdgeKind::{Param,Returns,Field}`, the df plane's bindings) form a
relation. A `WITH RECURSIVE` fixpoint over that relation propagates a known
type from where it is syntactically visible to where it is not:

```
step 0   let s: String = ...          s : String          (syntactic, given)
step 1   s.contains(x)                receiver(s) known   -> str::contains
step 2   fn f(a: String)              a : String          (param annotation)
step 3   f(s)                         argument flows      -> confirms
step 4   let t = s.clone()            returns Self        t : String
step 5   t.contains(y)                                    -> str::contains
step n   fixpoint, no new bindings
```

Terminates at a fixpoint, not a base case, same as `extract-graph-verb`'s
`--expand`. The recursive CTE over the sqlite export is the oracle the verb
must agree with, and it is the same mechanism here.

Partial is the point. A receiver whose binding is never syntactically typed
stays unclassified, which is the untyped-receiver decline law. The issue is
satisfied when SOME of the 195 move out of the undifferentiated bucket, with a
count that says how many and a reason per row that stayed.

### Where the rules live

`crates/sprefa-extract/AGENTS.md:19-30`: this crate is dl8's EDB producer,
facts here, resolution RULES in dl8 as `.dl7` programs. So the inference rules
are dl8's, and this crate's obligation is to emit the relations the rules need
plus a sqlite export the CTE can run against. Record which side each new
relation belongs to before writing either.

### Related

- `local-binding-inference`: the binding half of this, already filed
- `extract-graph-verb`: owns the recursive-CTE oracle and the fixpoint doctrine
- `ryi-stratify`: blocked on the graph verb
- `kind-vocab-constraint`: the substring defect this measurement was found through

## Acceptance Criteria

- [ ] the 2026-09-19 baseline above is reproducible by a committed script or test
- [x] a recursive fixpoint over fast-path facts assigns receiver types where a binding is syntactically typed
- [x] the fixture `contains` bucket splits: one `String`, one `Vec`, one unclassified
- [x] every unclassified fixture row carries `no_syntactic_receiver_type`
- [x] the fixpoint terminates and the DL8 test asserts a `new: 0` round
- [x] the String/Vec fixture split agrees with `--family scip` wherever SCIP returns a symbol
- [x] rules live in DL8 `.dl7` programs; this crate supplies facts and a recursive SQLite oracle

## Repro receipt

2026-09-26: commits `a7727d4a` (DL8 rules) and pending this worktree commit; `cargo test --manifest-path v8/Cargo.toml --test _10_receiver_type_inference -j 2` passes with resolved `str::contains` / `vec::contains`, an explicit untyped reason, and an asserted zero-new fixpoint round. `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run --features cli -j 2 --test all -E 'test(/^t_184_fast_recursive_receiver::/)'` passes; the SQLite recursive CTE yields one String, one Vec, and one untyped receiver row. A fixture `ryii scip --raw --scip-build` run returned distinct String and Vec symbols at the same two call spans. The 101-file 2026-09-19 baseline still lacks a committed reproducer; keep this issue open until that measurement is reproducible.

2026-09-27 current-tree check: `cargo nextest run --features cli --locked -j 2 --test all -E 'test(/^t_184_fast_recursive_receiver::/)'` passes the String/Vec/untyped fixpoint fixture (1/1). Applying current `ryii` build `7d418857` to the archived `9b25f783` source gives 25,368 `site` rows and 209 `contains` rows, so it does not reproduce the historical 24,873/195 counts. The archive has 102 `.rs` paths; excluding data-only `src/lang/0_call_kinds.rs` gives 101 input source files. The historical binary replay command is:

```sh
baseline=/Users/chrishafley/.cache/lanes/the-gang-graph/fast-recursive-old-crate
mkdir -p "$baseline"
git archive 9b25f783 Cargo.toml crates/sprefa-extract crates/soopy crates/hafley-observe | tar -x -C "$baseline"
CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo build \
  --manifest-path "$baseline/crates/sprefa-extract/Cargo.toml" \
  --locked --offline --features cli --bin ryi -j 2
(cd "$baseline/crates/sprefa-extract" && \
  /Users/chrishafley/.cache/boop/cargo-target/debug/ryi --family call src)
```

The offline historical build resolved 489 targets; it was stopped at 186 while compiling to honor the current CPU limit. The baseline reproduction remains unchecked and the issue stays open.
