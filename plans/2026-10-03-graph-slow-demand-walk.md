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

## Open

- Which relation sprefa rules emit into `extra` (item ids vs spans) is decided when
  sprefa emits SQL; the seam takes item handles.
- Macro-expanded calls: walk the expansion RA gives for the body (sema already sees
  it); no text scan.
