# graph --slow: demand-driven walk instead of eager whole-project inference

## Recon (2026-10-03, release ryii at cb404a5e, crates/hafley_scm/src)

| metric | value |
| --- | --- |
| `.rs` files | 152 |
| lines | 68,840 |
| `fn` declarations (grep) | 2,306 |
| edges answered for `--from lib.rs#build` | 27 |
| wall / cpu / peak RSS (slow) | 18.8 s / 40.8 s / 2.62 GB |
| wall / peak RSS (fast) | 0.50 s / 191 MB |

Path today: `0_graph.rs::run` -> `load_store` -> `slow_project_with_raw` (all planes,
all files) -> `checker_facts` -> `resolve_project` -> `rust_checker_ra::answer` ->
rayon over every file -> `walk_file` -> `Semantics` resolve on every call node of every
body. The question is answered afterwards by SQL over the full store.

Sample (60 s, all threads): `hir_ty::infer::InferenceContext::infer_expr_inner` 237k,
`infer_return` 43k, `check_call_arguments` 42k, `rust_checker_ra::walk_file` 42k,
`InferenceResult::for_body` 38k, `resolve_method_call` 17k, tracing_subscriber span
bookkeeping ~8k top-of-stack. The cost is type inference of every body in the corpus,
independent of the question: 27 edges needed, 2,306 functions inferred.

## Type sigs

```rust
// hafley_scm::read::lang (new numbered file next to 8e_rust_checker_modules.rs)
struct Walk<'db> {
    sema: Semantics<'db, RootDatabase>,
    project: HashSet<hir::Crate>,           // workspace members; everything else is extern
    seen: HashSet<hir::DefWithBody>,
    queue: VecDeque<(hir::DefWithBody, u32 /* depth */)>,
    edges: Vec<WalkEdge>,
    extra: &'db dyn Fn(hir::DefWithBody) -> Vec<hir::DefWithBody>,   // sprefa rule edges
}

struct WalkEdge { from: hir::DefWithBody, to: Target, kind: EdgeKind, site: TextRange }
enum Target { Project(hir::DefWithBody), Extern(hir::ModuleDef) }
enum EdgeKind {
    Call,          // f(..), path call
    Method,        // recv.m(..), resolved by inference of THIS body only
    Passed,        // project fn item / closure handed to an extern call as a value
    TraitImpl,     // project type flows into an extern call whose bound names trait T:
                   //   the project's impl T for that type becomes reachable
    Rule,          // from `extra` (sprefa)
}

fn seed(tree: &RustModuleTree, anchor: &str /* [PATH#]NAME */) -> Vec<hir::DefWithBody>
fn walk(w: &mut Walk, seeds: Vec<hir::DefWithBody>, max_depth: Option<u32>, deadline: Deadline)
fn step(w: &mut Walk, body: hir::DefWithBody, depth: u32)
//   for each call / method call / path expr in body's source only:
//     resolve with sema (infers this body; salsa caches it)
//     target in project crate      -> edge Call|Method, enqueue target
//     target extern                -> edge to Extern(target), do not descend
//       for each argument of that extern call:
//         type is FnDef / closure of a project item -> edge Passed, enqueue
//         type is a project ADT and the callee's generic bound names trait T
//           -> each project `impl T for ADT` method: edge TraitImpl, enqueue
//   for t in (w.extra)(body): edge Rule, enqueue t
fn rows(w: &Walk) -> impl Iterator<Item = GraphRow>   // same graph_node / graph_path rows
```

## Lifetimes

- RA host: loaded once per process (cleave's `Tier::Names` load path plus inference);
  in daemon mode it stays warm across questions.
- `Walk`: one per question. Bodies inferred = bodies popped; salsa memoizes inference,
  so a warm host answers a repeated or overlapping question from cache.
- Deadline: checked per popped body (the timeout covers the walk; load stays bounded
  by the daemon being warm).
- `extra`: provided by the caller; empty until sprefa rules feed it.

## Storage, reads, writes

- In memory per question: `seen`, `queue`, `edges`. No whole-project fact store is
  built for walk questions (`--from`, `--call-path`, `--callers` from a seed set).
- `--callers NAME` is the reverse question: it needs incoming edges, which a forward
  demand walk cannot give without an index. Options: (a) keep eager for `--callers`;
  (b) candidate bodies = bodies whose text names NAME (fast tier's syntax index), then
  infer only those. (b) matches the demand rule; text index is a prefilter, the checker
  decides.
- Optional later: persist `edges` per (content id of file, body) so a cold process
  reuses them; ids interned per .claude/skills/2026-10-03-sqlite-interning.

## Acceptance

1. Same edge set as today's eager slow walk on the stress lab's 71 anchors
   (bench/labs/lab-20261002-ryi-graph-fast-vs-slow), excluding new Passed/TraitImpl
   edges, which are reported separately.
2. `--from lib.rs#build crates/hafley_scm/src`: bodies inferred (counter) and wall/RSS
   before vs after.
3. hafley-observe growth test: bodies inferred grows with reached bodies, constant in
   unrelated corpus size (add N unrelated files, count stays).

## Phase 1 as built (2026-10-03, branch feature/graph-demand-walk)

| piece | file |
| --- | --- |
| provider: `WalkSession` (warm Slow host, `open`, `prime`, `seeds`) and `WalkSession::body_edges(body) -> BodyEdges` (one body in, its edges out) | crates/hafley_scm/src/read/lang/8f_rust_checker_body_edges.rs |
| worklist: `demand_walk(&WalkQuestion) -> WalkAnswer`, a thin BFS caller of `body_edges`, deletable once a sprefa program drives the provider | crates/hafley_scm/src/read/lang/8g_rust_checker_walk.rs |
| `graph --slow --from/--call-path` routing, rows | crates/sprefa-extract/src/0c_graph_walk.rs |
| `deadline::within` takes `Option<InterruptHandle>` (the walk has no SQLite) | crates/sprefa-extract/src/bin/ryi/0b_deadline.rs |
| growth test (`rust_walk.body` span, `assert_growth_sized` Constant, 2 -> 200 unrelated files) | crates/sprefa-extract/tests/196_rust_walk_growth.rs |
| lab (eager vs walk, `--from`, Rust anchors) | crates/sprefa-extract/bench/labs/lab-20261003-graph-slow-demand-walk |

Routing: the walk answers when `--slow`, arm `--from` or `--call-path`, no `--at`,
no `--scip-index`, no `--sqlite`, and the seed is Rust: `PATH#NAME` with PATH ending
`.rs`, or bare NAME over inputs whose call languages are all Rust (markdown, data,
fallback files allowed). Everything else (`--callers`, `--uses`, `--type-path`,
`--flow-path`, TS seeds, a bare NAME over a mixed-language corpus, `--at/--compare`,
`--sqlite`, `--scip-index`) still runs the eager `load_store`.

Behavior as built:

- Host: the Slow tier load (`checker_workspace(root, Tier::Slow, ..)`, sysroot and
  deps), one per process. Seeds come from that host: the seed file's parse, every
  `fn NAME` it writes, `sema.to_def`. The Names-tier `RustModuleTree` is a second RA
  load without sysroot, so seeding does not go through it.
- The seeds' crate closure is primed with the existing `prime_crate_closure`
  (def maps and trait impls, in parallel) before the first body; the first body
  otherwise builds the same def maps on one thread.
- Project = target declared in a supplied file of a workspace-member crate
  (`CrateOrigin::Local`); anything else is an extern edge, recorded, not descended.
  This is the eager tier's resolve universe (supplied files) as well.
- Per popped body: method calls (`resolve_method_call`), path calls and record
  literals (`resolve_path`), the same three site shapes `rust_checker_ra::walk_file`
  resolves. Nested `fn` items are skipped (their own bodies); closures stay in the
  body that writes them. Calls inside macro invocations are not walked (eager parity).
- Deadline: `--timeout` becomes `WalkQuestion.timeout`; its clock starts at the first
  popped body (after the RA load and `prime_crate_closure`, as the eager tier's clock
  starts after its store build) and is checked per popped body. Expiry is
  `CheckerError::Deadline`, exit 3 through `deadline::within`.
- Rows: extern edges dropped (the eager store has no row for an extern answer), then
  the existing `first_discovery` over `(path, name)` nodes; `graph_path.witness`
  holds walk edge ordinals (1-based) in place of store `_row` ids. Format unchanged.
- stderr adds one line: `demand walk: N bodies inferred, a call, b method, c passed,
  d trait_impl, e extern edges`. A seed file that owns no module in the crate graph
  gets the eager tier's `tier.rust-analyzer declined` line; non-seed files are never
  read, so they are never reported.
- `Passed`: a project fn item written as an argument of an extern call (path expr).
  `TraitImpl`: a project ADT argument (references stripped) of an extern fn whose own
  generic params bound trait T -> every method of the project's `impl T for ADT`.
  Both one hop.
- No `extra` seam and no `Rule` kind (coordinator scope change, agreed with the sprefa
  session, 2026-10-03): rule edges do not flow into a ryi fixpoint. The type sigs
  above keep `extra`/`Rule` as the original design; the built shape replaces them
  with the standalone provider, so a sprefa program can own the fixpoint.
- `tracing::debug!(target: "rust_walk.edge", ..)` per edge
  (`RUST_LOG=rust_walk.edge=debug`, fields to_path, to_name, kind) is how the lab
  attributes walk-only nodes to kinds.

## Results

Machine shared with other agents' builds (load average 10-13 during runs); round 2 of
two interleaved rounds. RSS on macOS under memory pressure excludes compressed pages,
so `peak memory footprint` from `/usr/bin/time -l` is listed beside it.

| question | tier | wall_seconds | rss_mb | footprint_mb | rows | bodies_inferred |
| --- | --- | --- | --- | --- | --- | --- |
| `--from lib.rs#build crates/hafley_scm/src` | recon (cb404a5e) | 18.8 | 2620 | | 27 | 2306 fn (all) |
| `--from lib.rs#build crates/hafley_scm/src` | eager (f4e19b7d) | 21.5 | 1137 | 2771 | 27 | all |
| `--from lib.rs#build crates/hafley_scm/src` | walk | 5.96 | 1501 | 2017 | 31 | 16 |
| `--from 0_graph.rs#run crates/sprefa-extract/src` | eager (f4e19b7d) | 16.5 | 1377 | 2265 | 112 | all |
| `--from 0_graph.rs#run crates/sprefa-extract/src` | walk | 7.72 | 1661 | 2242 | 116 | 95 |

Walk phases on `lib.rs#build` (spans): RA load 1.2-1.4 s, `prime_crate_closure`
3.9-4.2 s, 16 bodies 0.12 s. The footprint floor (about 2.0 GB) is the loaded crate
graph plus def maps; it is the same with and without priming (2.04 GB vs 2.04 GB on
`lib.rs#build`, 2.11 GB vs 2.11 GB on `0_graph.rs#run`).

Row differences on the two acceptance questions, all walk-only:

| question | node | discovering edge |
| --- | --- | --- |
| lib.rs#build | query_ext_error.rs ScmppOnly, UnknownOperator, Arity; _0_types.rs Syntax | Call to a record-shaped enum variant (`V { .. }`) |
| 0_graph.rs#run | 0_graph.rs default, 0a_bind.rs default, 1fa_checker_edges.rs default | Call to a `#[derive(Default)]` method |
| 0_graph.rs#run | 1a_ts7_lsp_session.rs notify | Method, turbofish call `lsp.notify::<T>(..)` |

### Agreement on the stress lab's Rust call anchors (`--from`)

28 anchors (lab-20261002 names.tsv, lang rust, plane call); corpus `--root
crates/hafley_scm --pattern '**/*.rs' crates/hafley_scm` (the crate holds one `.mjs`;
without the pattern a bare NAME stays eager). Key = (path, name) as in that lab.
Script `0_run.py`, explanations `1_explain.py` against an eager store published with
`--sqlite`.

| total_anchors | agree | eager_only | walk_only | anchors_with_any_difference | exits_nonzero |
| --- | --- | --- | --- | --- | --- |
| 28 | 499 | 26 | 47 | 6 | 0 |

| anchor | agree | eager_only | walk_only |
| --- | --- | --- | --- |
| build | 104 | 0 | 23 |
| resolve | 332 | 25 | 20 |
| new | 25 | 0 | 2 |
| df_edge | 2 | 1 | 0 |
| parse | 8 | 0 | 1 |
| crates/hafley_scm/src/atoms.rs#new | 0 | 0 | 1 |
| the other 22 anchors | 30 | 0 | 0 |

Every eager-only node (26) is a `(path, name)` collapse in the eager walk: its
first-discovery chain passes a node whose name several fns of one file share, and
the eager walk expands all of them; the demand walk expands only the body the call
resolved to.

| collapse node | fn_count_in_file | eager_only_nodes |
| --- | --- | --- |
| read/scip.rs build (`resolve > scip_call_target > byte_range_cached > build_doc_spans > build > build_indexer ..`) | 7 | 23 |
| read/types.rs new (`.. > new > new`, reaching tsi/sink.rs new) | 5 | 2 (resolve, df_edge) |
| read/lang/ts.rs resolve (seed name shared) | 2 | rides the two above |

Walk-only nodes (47), by the edge kind that discovered them:

| cause | nodes | eager reason |
| --- | --- | --- |
| Call/Method to a derive-generated method (`default`, `clone`) | 22 | the syntax def index has no fn at the `#[derive]`; the checker answer is unjoined |
| Call to a record-shaped enum variant (`Arity`, `Syntax`, `Binding`, `Module`, `Namespace`, `Exited`, `Killed`, `Sql`, ..) | 11 | no CallF def node for `V { .. }` variants; unjoined |
| TraitImpl (`hash`, `cmp` of project ADTs, derived or written, handed to extern fns bounded by `Hash`/`Ord`) | 7 | no edge kind |
| Passed (fn item as an extern call argument: `target_kind`, `manifest_uses_parent_path`, `has_parent_component`, `from_env`, `sql`, `module_qualifier`) | 6 | no edge kind for a fn value |
| Call from a Passed-reached body (scip_ensure.rs `default`, called by `from_env`) | 1 | its caller is unreached in the eager walk |

Classification: lab `2_kinds.py` (first `rust_walk.edge` debug event into each node;
derive = no `fn NAME` in the file; variant = capitalized name).

## Open

- The provider's handle is `ra_ap_hir::DefWithBody` (salsa id, valid for the session's
  host revision). What a sprefa program passes back (item ids vs spans) is decided
  when sprefa drives `body_edges`.
- Macro-expanded calls: walk the expansion RA gives for the body (sema already sees
  it); no text scan. Not walked in phase 1 (eager parity).
- TODO TraitImpl: only bounds on the extern fn's own generic params. Impl-level
  bounds (`HashSet<T>::insert`, bound on the impl), trait-method paths
  (`ToString::to_string(&s)`, the bound is `Self: Trait`) and the receiver of an extern
  method call are not read; the bounded param is not matched to the argument position.
- TODO dyn / generic dispatch: a method call on a generic or `dyn` receiver resolves to
  the trait declaration; the project impls of that trait are not enqueued.
- TODO Passed: closures need no edge (they are inside the walked body); a fn item
  passed to a project fn (`apply(f)`) is not an edge, the callee's `f()` resolves to a
  local.
- `--callers` stays eager/targeted (reverse question; options (a)/(b) above).
- `--call-path` routes through the walk; the lab measured `--from` only.
- A bare NAME over a mixed-language corpus stays eager: the TS side would need its
  own seeds.
- Cost floor: `prime_crate_closure` (3.9-4.2 s) and the 2 GB footprint are the Slow
  load of the seeds' crate closure; a warm daemon host amortizes both.
