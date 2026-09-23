# Ryi end-state mission

Status: guiding target, not a claim that the listed gates pass. Baseline: `main` at
`b0600e7c`, 2026-09-23. Scope: **ryi and its `sprefa-extract` library**. The
maintained, cross-run fact store and DL7 rule programs belong to dl8; ryi owns
the one-shot producer, one-shot project analysis, wire, CLI, and proof that its
facts can feed that store. [Boundary law](../AGENTS.md#division-of-labor-with-dl8)

## Mission and done condition

For one project at one revision, ryi reads each file's bytes at that revision,
extracts syntax facts, and assembles a cross-file graph. TSI is its common type
interchange: **fast TSI** runs from parsing and bounded inference without a
compiler; **slow TSI** adds native-checker and SCIP evidence. Both name the same
project contents, relations, targets, witnesses, and explicit abstentions.
The CLI exposes those facts and useful one-shot queries. The same output can
generate D2 type graphs and collapsible Markdown without a separate parser.
[Existing TSI wire](../src/tsi/types.rs) · [Existing relation registry](../src/tsi/registry.rs)

The mission is done when all of these are measured:

1. A pinned `(repo, revision)` or explicit content snapshot yields one stable
   file set. Every emitted span names a blob in that set; a mismatched SCIP
   document is reported, never joined by path alone.
2. The fast path runs one tree-sitter parse and one bundled SCM query **per
   file where that grammar is the backing engine**. Other backing parsers remain
   explicit. TypeF, CallF, DfF, symbols, scopes, imports, and reference seats
   consume its named facts where their grammar expresses them. No second SCM
   pass reads the file. [Current separate SCM pass](../src/project.rs#L1277) ·
   [Current SCM file pass](../src/lang/7_scm_rows.rs#L341)
3. Fast TSI reaches a declared fixpoint or an explicit round/budget limit over
   the complete project fact set. Every claim has a rule and premise witness;
   ambiguous and unsupported references have reasons. It runs with SCIP and
   checkers disabled. [Fast inference card](../../../issues/fast-path-recursive-inference/item.md)
4. Slow TSI may consume SCIP and native checkers. SCIP schema fields and index
   instances have per-language coverage receipts, including external symbols;
   unclassified drops fail. A slow run declares relation coverage only where
   the producer can prove it. [SCIP conformance card](../../../issues/scip-ingestion-conformance/item.md)
5. A pinned corpus prints fast/slow comparable-unit coverage, correct target,
   wrong target, and abstention counts. The denominator is named and nonzero;
   no rule uses slow facts while producing the fast answer. Existing goldens
   and CLI/library reachability remain intact. [Ratchet](../tests/golden_parity.rs) ·
   [Capability parity](../tests/4_capability_parity.rs)
6. `ryi graph` and an export mode produce deterministic type-reference boards
   from ryi's own output. Cycles form SCCs before topological layering; source
   nodes retain identity and evidence. A Markdown export places each generated
   `d2` block inside `<details><summary>...</summary>`. D2 compilation and
   rendered layout are tested. [Current graph CLI](../src/0_graph.rs) ·
   [Current D2 example](../examples/typegraph_d2.rs) ·
   [D2 gate](../tests/15_typegraph_d2.rs)
7. Superseded Rust parsing/projection branches are removed only after their
   outputs match the chosen goldens and real-repository dogfood. Report net
   production Rust lines and parse/query counts per migrated language. The
   first generic-emission slice added 145 non-test Rust lines, so code reduction
   is a future measured outcome, not an achieved claim.

## Formula: producers, facts, tiers, consumers

```text
Snapshot S = (repository identity, revision, [(path, content digest, bytes)])
Fast(S)   = ExtractSCM(S) ∪ OtherParserFacts(S) ∪ InferN(ExtractedFacts(S))
Slow(S)   = NativeChecker(S) ∪ SCIP(S), admitted only with matching content
TSI(S)    = facts + run(mode, tool, scope) + witness + coverage + diagnostic
Graph(S)  = project-wide joins and recursive closure over TSI/family facts
View(S)   = JSONL | SQLite | graph query | D2 | Markdown-with-D2
```

SCIP is an input to slow TSI and an offline oracle for fast TSI. Fast TSI has no
SCIP runtime dependency. The [TSI protocol](../src/tsi/types.rs) already has
`Syntax`/`Semantic` runs, content-digest scope, per-fact witnesses, coverage,
and diagnostics. The [registry](../src/tsi/registry.rs#L66) already has the
optional `tsi.scip_symbol` bridge. Native checkers currently enumerate semantic
rows through [`SemanticRows`](../src/tsi/semantic.rs); SCIP-to-TSI coverage is
not yet proved equivalent to complete SCIP ingestion.

The type-reference evidence ladder is a set of claims, not a forced overwrite:

| Claim | Input | Output evidence | Stop condition |
| --- | --- | --- | --- |
| Written | CST/SCM capture | spelling, path, blob, span, owner | parse error or absent seat |
| Fast candidate | local type, binding, scope, import facts | candidate target set and rule | zero or multiple candidates |
| Fast resolved | project join or inference round | target plus premises, round, origin | ambiguity or budget |
| Slow witnessed | SCIP occurrence/symbol or checker relation | source identity, target, coverage | missing/mismatched index |
| Derived graph | accepted TSI edge claims | SCC, topo layer, traversal depth | explicit graph budget |

Coverage is per run and relation. A syntax run remains partial as the
[wire contract](../src/tsi/types.rs#L64) requires. A slow disagreement and a
fast abstention are retained as separate evidence, including when a compiler
identifies an external symbol.

## One projected task pipe

Pseudo-RxJS describes sequencing and ownership, not a new runtime dependency:

```ts
const mission$ = snapshot(repo, revision).pipe(
  mergeMap(file => parseOnce(file).pipe(map(tree => emitNamedFacts(tree)))),
  toArray(),
  map(facts => assembleProject(facts)),
  expand(state => state.delta.length && state.round < maxRounds
    ? of(inferOneRound(state)) : EMPTY),
  takeLast(1),
  switchMap(fast => optionalScipAndCheckers(fast.snapshot).pipe(
    map(slow => witnessTsi(fast, slow)))),
  map(tsi => queryAndDraw(tsi, condenseScc, topoLayers, d2Boards))
)
```

`optionalScipAndCheckers` does not gate fast emission. The inference rounds
read the newly derived delta, append supported facts, and terminate on an
empty delta or report the explicit limit. Ryi may execute a one-shot version;
dl8 owns the maintained rules and incremental evaluation. One rule source of
truth must drive both, or the two evaluators need a parity test.
[Ryi/dl8 ownership](../AGENTS.md#division-of-labor-with-dl8) ·
[Current one-shot graph](../src/0_graph.rs)

## Type signatures and instance timeline

Current seams are [`Family`](../src/types.rs#L170),
[`Project<F>`](../src/types.rs#L1864),
[`Source`](../src/types.rs#L2725),
[`Resolve<F>`](../src/types.rs#L2359), and
[`BlobSource`](../src/types.rs#L1876). `Family` keeps its associated node,
edge, and aux vocabularies; `Project<F>` remains the per-file typed projector.
The proposed changes are deliberately concentrated at three seams:

```rust
// Existing per-file source, extended to share one query run with all projectors.
fn extract(&self, path: &str, bytes: &[u8], mask: FamilyMask) -> RyiOutput;
// body: parse backing tree once; run query pack once; project named facts;
//       return owned bundles and source facts; drop parser arena afterward.

// Existing source-agnostic reader, given a pinned snapshot by its caller.
fn blob(&self, path: &str) -> Option<Vec<u8>>;
// body: read exactly the revision selected by the snapshot; match digest.

// Proposed one-shot orchestration functions; no extra provider trait presumed.
fn infer_fast_tsi(facts: &ProjectFacts, max_rounds: u32) -> TsiRun;
// body: delta rounds; emit provenance and abstentions; report limit/fixpoint.
fn project_scip_tsi(index: &ScipIndex, snapshot: &Snapshot) -> TsiRun;
// body: validate document digests and coverage; retain external symbols.
fn type_boards(tsi: &TsiRun, entry: NodeKey, budget: usize) -> Vec<D2Board>;
// body: canonicalize graph; SCC; topo layers; chunk; preserve cross-board edges.
```

These are target signatures for review, not declarations already in the tree.
Before implementation, decide whether `RyiOutput` gains an owned named-fact
field or whether its existing family bundles directly consume the single
`MatchArena`. Preserve `Source` implementations and `Resolve<F>` output while
swapping their inputs, then delete old projection code only with parity proof.
Implementing SCIP's `SemanticRows` contract can reuse the existing trait;
introduce a provider trait only if two concrete producers need the same new
method. [SCM emission](../../hafley_scm/docs/0_emit.md) ·
[Current Kotlin projection](../src/lang/6_scm_family.rs)

Instance lifetime and storage sequence for one invocation:

1. Snapshot owner fixes `(repo, revision)`, canonical path set, and content
   digests. Plain-directory runs record a content-scope digest; they do not
   claim a Git revision. `RunOut.scope` names exactly the bytes read.
2. Each file worker owns bytes, parser arena, tree, and `MatchArena`; it writes
   owned phase-1 facts. The tree and arena die with that file task. No tree
   pointer enters TSI or the project index.
3. The project invocation owns the collection of file facts and one
   [`ProjectCx`](../src/types.rs#L1885). It builds definition/path/module
   indexes once, then `Resolve<F>` and bounded inference read those indexes
   and append witnessed claims. A claim key includes snapshot, path, blob,
   relation, source span, and target identity; repeated witnesses attach to
   that claim rather than duplicating it. The exact key is a test fixture.
4. The slow adapter reads a SCIP index and matching snapshot bytes. It writes
   its own TSI run, facts, witnesses, coverage, and diagnostic rows. The TSI
   [ingest door](../src/tsi/ingest.rs) validates relations and canonicalizes
   run-local ids/ordinals; cross-run identity comes from spans and symbols.
5. CLI serializers read the finished one-shot state. A `--state` export may
   publish that invocation's SQLite database; maintained cross-run storage,
   invalidation, and eviction remain dl8's work.

## Execution order and merge gates

Each row is a short mergeable slice. Preserve existing aliases, tests, and
families; replace an implementation only after equivalent output is measured.

| Slice | Work and existing issue | Receipt before main |
| --- | --- | --- |
| 0. Baseline | Pin one ryi self-snapshot, current parse/query counts, TypeF/TSI row counts, and D2 example output. [Example](../examples/typegraph_d2.rs) | Reproducible command and content digest; no changed facts. |
| 1. SCIP coverage | Break down [conformance](../../../issues/scip-ingestion-conformance/item.md) into per-language fixture and field/instance receipts. Track unreadable documents, range-conversion drops, and external symbols in raw and `--family scip` output. | Zero unclassified drops, explicit waivers, nonzero per-language coverage. No 100% claim before this. |
| 2. One query pass | Feed phase-1 and `scm_facts` from the same file query arena; extend generated named relations through TypeF, scope, reference, and import seats. [Current duplicate pass](../src/project.rs#L1277) | Existing goldens and 42-file Kotlin `ryi fast` parity; count one tree-sitter parse/query per supported file. |
| 3. Snapshot join | Bind fast facts, SCIP documents, checker answers, and TSI runs to one path/content scope. [Current request](../src/project.rs#L98) | Mismatched bytes produce a diagnostic; same-snapshot cross-file fixture resolves. |
| 4. Fast TSI rounds | Build the minimum missing binding/initializer/receiver facts, then one-shot N-round inference with rule and premise witnesses. [Local binding](../../../issues/local-binding-inference/item.md) · [Recursive inference](../../../issues/fast-path-recursive-inference/item.md) | Fixpoint/budget asserted; `contains` bucket splits; abstention reasons counted; compiler disabled. |
| 5. Slow TSI and score | Map conformed SCIP and native checker facts into TSI semantic runs; compare same-unit claims with fast, including external and unmatched coverage. [Fast/slow contract](../../../issues/extract-fast-slow-trait-divide/item.md) | Pinned nonzero denominators and correct/wrong/abstain tables on self and language fixtures. |
| 6. CLI and D2 | Reuse `ryi graph`'s one-shot views and move the D2 example's algorithm behind a CLI export. Compute SCC DAG tiers, retain hop distance as a separate field, and emit `.d2` or collapsible Markdown. [Graph CLI](../src/0_graph.rs) · [D2 gate](../tests/15_typegraph_d2.rs) | D2 compiles, boards are stable across runs, cycles and cross-board edges visible, ryi graphs its own `RyiOutput`. |
| 7. Remove overlap | Delete superseded hand-written syntax projection paths, keep mutation/rehome paths that need more than facts, and update issue status from measured receipts. | No lost feature, golden or CLI capability; net Rust and parse/query counts reported. |

The SCIP coverage work is required before a numeric **accuracy** claim. SCM
single-pass work and syntax-only TSI can proceed without a SCIP index at
runtime. The plan has no target test-count expansion: add a focused assertion
to an existing gate when it already covers the behavior. The crate gate is
`cargo test --features cli` from this crate, with environment-dependent tests
reported by name rather than described as green.
[Test topology](../AGENTS.md#pointers) · [CLI parity gate](../tests/4_capability_parity.rs)

## Generated Markdown D2 contract

The marked block below is regenerated from ryi's real TypeF nodes and
`Resolve<TypeF>` output. Run from `crates/sprefa-extract`:

```sh
cargo run --example typegraph_d2 -- --root src --entry src/types.rs::RyiOutput --out /tmp/ryi-typegraph --markdown-into plans/0_ryi_end_state_mission.md
```

The current example's bands are BFS distance, not SCC/topological layers.
The end-state export adds snapshot, evidence tiers, and coverage to the summary.
[Example implementation](../examples/typegraph_d2.rs) ·
[Layout test](../tests/15_typegraph_d2.rs)

<!-- ryi:typegraph-d2:start -->

<details>
<summary>Ryi type graph: src/types.rs::RyiOutput, board 1/3</summary>

```d2
direction: down

src_types_rs__CallF: CallF { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__CstF: CstF { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__DataF: DataF { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__DfF: DfF { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__FamilyBundle: FamilyBundle { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__RyiOutput: RyiOutput { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__Strings: Strings { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__TypeF: TypeF { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }

src_types_rs__Strings -> src_types_rs__Strings: uses
src_types_rs__CstF -> src_types_rs__CstF: uses
src_types_rs__TypeF -> src_types_rs__TypeF: uses
src_types_rs__CallF -> src_types_rs__CallF: uses
src_types_rs__DfF -> src_types_rs__DfF: uses
src_types_rs__DataF -> src_types_rs__DataF: uses
src_types_rs__FamilyBundle -> src_types_rs__FamilyBundle: uses
src_types_rs__RyiOutput -> src_types_rs__Strings: field
src_types_rs__RyiOutput -> src_types_rs__CstF: field
src_types_rs__RyiOutput -> src_types_rs__TypeF: field
src_types_rs__RyiOutput -> src_types_rs__CallF: field
src_types_rs__RyiOutput -> src_types_rs__DfF: field
src_types_rs__RyiOutput -> src_types_rs__FamilyBundle: field
src_types_rs__RyiOutput -> src_types_rs__DataF: field
```

</details>

<details>
<summary>Ryi type graph: src/types.rs::RyiOutput, board 2/3</summary>

```d2
direction: down

src_types_rs__Edge: Edge { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__Family: Family { shape: rectangle; style.fill: "#4b2e83"; style.font-color: "#ffffff" }
src_types_rs__NameId: NameId { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__Node: Node { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }

src_types_rs__NameId -> src_types_rs__NameId: uses
src_types_rs__Node -> src_types_rs__NameId: field
src_types_rs__Node -> src_types_rs__Family: generic
src_types_rs__Node -> src_types_rs__Node: uses
src_types_rs__Edge -> src_types_rs__Family: generic
src_types_rs__Edge -> src_types_rs__Edge: uses
```

</details>

<details>
<summary>Ryi type graph: src/types.rs::RyiOutput, board 3/3</summary>

```d2
direction: down

src_types_rs__NodeRef: NodeRef { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }
src_types_rs__Span: Span { shape: rectangle; style.fill: "#1f4e79"; style.font-color: "#ffffff" }

src_types_rs__Span -> src_types_rs__Span: uses
```

</details>

<!-- ryi:typegraph-d2:end -->

For an actual type board, node identity must include path and declaration
coordinate or canonical symbol identity. The current example keys by
`(path, name)`, uses BFS distance bands, and scans every Rust file twice through
`node_kinds` and `resolve_project`. Its rendered D2 receipt proves the export
path exists, not the finished topology algorithm.
[Current identity and traversal](../examples/typegraph_d2.rs#L14) ·
[Current test limits](../tests/15_typegraph_d2.rs#L75)

## Decisions left explicit

- Specify the fact key and versioned TSI extension for inference premises,
  rule name, round, abstention, and snapshot identity before implementing the
  first round. Existing `Method` and `CoverageOut` must retain their meanings.
- Choose the one rule source of truth for ryi's one-shot inference and dl8's
  maintained evaluation. A shared program or a parity test is required.
- State which SCIP fields get a recorded waiver; exact ingestion means every
  field and occurrence is accounted for, including a documented waiver.
- Measure parser counts per backing engine. "One parse" applies to a reused
  tree-sitter tree, not to a native checker or a different parser required by
  another family.
