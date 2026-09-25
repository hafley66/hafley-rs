# sprefa-extract — Agent Guidance

One source file -> flat graph facts (JSONL). Phase-1 only: per-file,
parallel, pure, cacheable. No daemon, no network.

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
| maintenance | dl8 `_6_eval` + `sqlite_ivm` | a blob delta re-derives only dependent edges, incrementally, across runs |
| commit-to-commit delta | this crate, one shot (`extract diff --from A --to B`, issue `extract-diff-verb`) | soopy snapshots at two revisions, `diff_snapshots`, phase 1 on the changed blobs, resolve at both ends, set-difference keyed by (path, names, kind, origin) |
| oracle | scip slow lane here, graded by a dl8 rule | `RATCHET.tsv` is a query result |

`Resolve<CallF>` in this crate is the hand-compiled fast path; the DL7 rule
set is its spec. Composition with soopy (revision snapshots, blob ids) is
this crate's to use freely. Every invocation is one process that reads and
writes files and exits; there is no resident daemon, that is dl8's layer.
A one-shot delta between two commits is in scope (user-set 2026-09-18:
"extract on its own very capable"); a maintained, incremental one is not.
`extract watch` emits blob deltas and stops there.

One-shot traversal over a single resolve pass is in scope on the same
grounds (user-set 2026-09-18: "extract must be as capable as possible ...
we will host or re-use it in dl8 later"). That covers reachability from an
entrypoint, reverse edges, type usage, and `--expand` closing the file set
to a fixpoint. `ryi graph` keeps its fact store at
`<root>/.dl/.state/graph-<key>.db`, stamped with a `corpus` row (ryi build,
tier, arms, root, HEAD, `git status --porcelain=v2` hash) and per-file
digests. Each run compares HEAD and the status hash, re-hashes only the
paths git lists, reports changed/moved/added/removed files, re-extracts
when stale, answers, and exits. `--db PATH` queries a store built by
`fast`/`slow --sqlite`. A maintained dead-code or liveness view is not in
scope: those stay PROGRAMS under the map below, and incremental
re-derivation across runs is dl8's.

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
