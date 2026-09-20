---
created: 2026-09-20
updated: 2026-09-20
type: feature
status: open
priority: normal
epic: ryi-new-verbs
labels: [extract, artifact-cli]
---

## Description

New verb module `crates/sprefa-extract/src/0_graph.rs` (NEW) carries the `--callers` arm and the shared `GraphCx` plumbing every later graph arm reuses: one `resolve_project` pass, a `(path, name)` visited set, and the `Grade` mapping from `resolution_origin`. `crates/sprefa-extract/src/bin/ryi.rs` gets a `#[path = "../0_graph.rs"] mod graph;` alongside the existing `mod diff;` / `mod source_move;` block at lines 50-69, and a `Some("graph")` string-match dispatch arm in `run()` next to the `move`/`rename`/`diff` blocks at lines 581-613. Three new `FlatFact` variants land in `crates/sprefa-extract/src/types.rs` next to `ResolvedEdge`/`ResolvedTypeEdge` (line 3327 area): `GraphNode`, `GraphEdge`, `GraphRoot`, each carrying a `grade: String` column (`"+"`/`"~"`/`"-"`). The matching TypeSpec records `graph_node`, `graph_edge`, `graph_root` land in `crates/sprefa-extract/schema/1_facts.tsp` next to `ResolvedEdge` (line 64), each with a `grade: string` column, so the generated sqlite writers in `crates/sprefa-extract/src/bin/ryi/0_sqlite.rs` pick them up without a hand-written table. `--callers NAME` walks `resolved_edge` rows where `callee_name == NAME`, reversed so the callee becomes the traversed node and the caller becomes the edge target, and prints the `+`/`~`/`-` split as a summary line.

## Signatures

```rust
// crates/sprefa-extract/src/0_graph.rs (NEW)

/// Shared traversal context: one resolve pass, held for every graph arm in
/// one process. dl8 owns row storage (AGENTS.md:39-47); this is a cursor
/// over one in-memory resolve, never a cache.
pub struct GraphCx {
    facts: Vec<FlatFact>,                  // resolve_project(&ResolveRequest{..}), once
    visited: std::collections::BTreeSet<(String, String)>, // (path, name); first depth wins
}

impl GraphCx {
    pub fn load(paths: &[PathBuf], arms: ResolveArms) -> Result<GraphCx, Box<dyn std::error::Error>>;
    fn resolved_edges(&self) -> impl Iterator<Item = &FlatFact>; // filters FlatFact::ResolvedEdge
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade { Plus, Tilde, Minus }

impl Grade {
    /// Anchors from extract-graph-verb's Grades table (module_plane = `+`,
    /// unresolved = `-`); every other resolver leg (corpus_unique, checker,
    /// alias_chain, param, receiver, self_type, iface_impl, decorator,
    /// subscript, return_call, scip, same_file) answered by a heuristic
    /// match rather than a followed import, so it grades `~`.
    pub fn from_origin(origin: &str) -> Grade {
        match origin {
            "module_plane" => Grade::Plus,
            "unresolved" => Grade::Minus,
            _ => Grade::Tilde,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self { Grade::Plus => "+", Grade::Tilde => "~", Grade::Minus => "-" }
    }
}

#[derive(Default)]
struct GradeSplit { plus: u32, tilde: u32, minus: u32 }

pub fn run_callers(cx: &GraphCx, name: &str) -> Vec<FlatFact>; // graph_edge rows, reversed

// pseudo-code
fn run_callers(cx, name):
    let mut edges = Vec::new()
    let mut split = GradeSplit::default()
    for edge in cx.resolved_edges():
        if edge.callee_name == Some(name):
            let grade = Grade::from_origin(&edge.resolution_origin)
            split.bump(grade)
            edges.push(FlatFact::GraphEdge {
                from_path: edge.callee_path.clone(), from_name: edge.callee_name.clone(),
                to_path: edge.caller_path.clone(),   to_name: edge.caller_name.clone(),
                kind: "call".into(), grade: grade.as_str().into(),
            })
    emit_each(&edges)
    emit_summary_line(&split)   // "N edges: n+ +, n~ ~, n- -"
    edges
```

```rust
// crates/sprefa-extract/src/types.rs — new FlatFact variants, next to ResolvedEdge (~line 3327)
#[serde(rename = "graph_node")]
GraphNode { path: String, name: Option<String>, depth: u32, grade: String, line: Option<u32> },

#[serde(rename = "graph_edge")]
GraphEdge { from_path: String, from_name: Option<String>, to_path: String, to_name: Option<String>, kind: String, grade: String },

#[serde(rename = "graph_root")]
GraphRoot { path: String, name: Option<String>, span: Option<SpanOut>, found: bool },
```

```typespec
// crates/sprefa-extract/schema/1_facts.tsp — next to ResolvedEdge (line 64)
model GraphNode { ...ExportRow; record: "graph_node"; path: string; name: string | null; depth: uint32; grade: string; line: uint32 | null; }
model GraphEdge { ...ExportRow; record: "graph_edge"; from_path: string; from_name: string | null; to_path: string; to_name: string | null; kind: string; grade: string; }
model GraphRoot { ...ExportRow; record: "graph_root"; path: string; name: string | null; span: Extract.SpanOut | null; found: boolean; }
```

## Fixture

`tests/fixtures/ts5_findings/module_plane/`. Every import in the directory is a relative specifier (`grep -rn 'from ["\']' tests/fixtures/ts5_findings/module_plane/*.ts | grep -v '"\./''` returns nothing outside `./`), so it is closed under `resolved_import`: no edge leaves the directory for node_modules or an absolute path. It also carries real cross-file call chains (`two_hop_consumer.ts -> two_hop_outer.ts -> two_hop_middle.ts -> two_hop_inner.ts:deep`) and a re-export cycle (`cycle_a.ts` <-> `cycle_b.ts`, consumed by `cycle_consumer.ts:walk`), which exercises the `GraphCx` visited set this card introduces.

`tests/fixtures/deps` (the task's suggested first check) was rejected: `app.ts` imports only `const` bindings (`exact`, `emitted`, `boxed`, ...) with zero function calls, so it produces zero `resolved_edge` rows and cannot golden a `--callers` arm.

Command: `ryi graph --callers deep tests/fixtures/ts5_findings/module_plane`

## Acceptance Criteria
- [ ] `extract graph --callers NAME` returns reverse edges with the same grade column
- [ ] every `graph_node` and `graph_edge` row carries `grade`; a summary line prints the `+`/`~`/`-` split
- [ ] a golden over a fixture directory closed under `resolved_import`, not the instant corpus
- [ ] `cargo test --features cli --no-fail-fast` green from `crates/sprefa-extract`

## Tests Run

## Implementation Notes

## Comments

## Decisions
