---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: deferred
priority: normal
epic: ryi-fast-tier
related: ['@local-binding-inference', '@extract-graph-verb', '@kind-vocab-constraint']
labels: [extract, intent-architecture]
blocked_by: ['@scip-ingestion-conformance']
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
- [ ] a recursive fixpoint over the fast path's own facts assigns receiver types where a binding is syntactically typed
- [ ] the `contains` bucket splits, with a count per resolved receiver type and a count that stayed unclassified
- [ ] every unclassified row carries a reason, no silent drops
- [ ] the fixpoint terminates and the termination is asserted, not assumed
- [ ] the split agrees with `--family scip` over the same corpus wherever scip has an answer
- [ ] the rules live on the side `AGENTS.md:19-30` assigns them, and the issue records which
