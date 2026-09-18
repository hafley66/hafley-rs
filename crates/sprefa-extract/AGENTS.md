# sprefa-extract — Agent Guidance

One source file -> flat graph facts (JSONL). Phase-1 only: per-file,
parallel, pure, cacheable. No daemon, no database, no network, no watchers.

## The boundary law

Extract answers **what is written**. Programs answer **what it means**.
Framework knowledge (hook naming conventions, RTK codegen patterns, ORM
idioms) NEVER enters this crate — it lives one layer up as datalog programs
over the facts. The three layers: (1) INDEX here, (2) DERIVE in the
store/engine, (3) PROGRAMS in `.dl` scripts. Every family/facet in this
crate earns its place by a program that needs it.

The shape is not new: extract -> fact store -> query programs is the CodeQL
architecture; cst+df+call as merged graph planes is Joern's Code Property
Graph. Steal from both literatures freely.

## Division of labor with dl8 (user-set 2026-09-18)

`~/projects/sprefa` is dl8: DL7 compiled to Rust, `_6_eval` stratified
semi-naive, `sqlite_ivm`. It is the parent; this crate is its EDB producer.

| layer | owner | job |
| --- | --- | --- |
| EDB | this crate, phase 1 (`extract watch` retract/assert receipts, blob-keyed) | facts: `call_site`, `def`, `import`, `receiver_binding`, module indexes |
| rules | dl8 `.dl7` programs | `resolved_edge(site, def, origin) :- leg(...)`, one rule per `ResolutionOrigin` variant, stratified in the order the Rust arms try them |
| maintenance | dl8 `_6_eval` + `sqlite_ivm` | a blob delta re-derives only dependent edges |
| oracle | scip slow lane here, graded by a dl8 rule | `RATCHET.tsv` is a query result |

`Resolve<CallF>` in this crate is the hand-compiled fast path; the DL7 rule
set is its spec. Do not build a watcher, a daemon, a delta resolver, or a
persistent index in this crate: that is dl8's layer. The `extract watch`
verb emits deltas and stops there.

## Analysis family map (program vs facet vs rabbit hole)

PROGRAMS over already-emitted facts (zero new extraction — do NOT add
extractor code for these):
- dead code / liveness (dead stores = one df query)
- taint (source->sink + sanitizers = derived flow + endpoint annotations)
- typestate / API-protocol ("open before read"; rules-of-hooks IS one)
- metrics/architecture (coupling, cohesion, cyclomatic)
- effect/purity (call-graph reach to an effectful-API list)

ONE NEW FACET, then a program (facet work here is legitimate):
- control dependence (dominance from cst) -> program slicing
- pointer/alias (Doop is the datalog-native lineage; own arc, human-gated)
- abstract interpretation lite (const facet is already baby constant prop)
- escape/capture (lam_sym closures already carry captures)

ADJACENT UNIVERSES (know, don't build):
- symbolic execution (SMT religion), shape/heap analysis (TVLA),
  concurrency/races (cheap slice only: lock-order + await-across-lock),
  termination (the engine guarantees it)

THE TRAP: big-O. Precise static complexity is unsolved. Heuristics only
(loop depth x call-graph cycles x input-size params); label them
heuristics or they lie.

PRIORITY: taint + slicing are the two highest-value next programs (they
ride derived inter-procedural flow + control dependence). Typestate is the
everyday workhorse. Everything else: programs, NOT extractors.

## Inter-procedural rule (permanent)

Intra-procedural extraction ONLY — the per-file purity is what keeps this
crate parallel and incremental. arg->param / ret->call-res flow is DERIVED
in the engine from df_args/df_param_pos + resolve edges: `flow_edges`
(`src/types.rs`) emits `FlowEdgeKind::{ArgToParam, RetToCallRes}` today;
`LambdaElem` / `LambdaRet` are reserved in the vocabulary and never
constructed. Eager whole-repo context-sensitive extraction
is the IFDS trap: never queued, never start it.

## Pointers

The working plan (increment briefs I0a-I7, design seeds S1-S4, gates,
conventions, recovery state) lives in the worktree at
`v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md` — read it before
any non-trivial change here. Hard rules: V5 IS CORRECT (parity against
captured oracles), two-lane rows (ported asserted byte-exact / v6-only
reported never asserted), no new deps without adjudication,
`cargo test --features cli` is the gate, UPDATE_SNAP forbidden unless the
increment declares it.
