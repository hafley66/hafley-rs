---
created: 2026-09-20
updated: 2026-09-20
type: feature
status: open
priority: normal
epic: ryi-new-verbs
labels: [extract, artifact-cli]
blocked_by: ['@graph-callers-arm']
---

## Description

Adds the `--from PATH:NAME [--expand]` arm to `crates/sprefa-extract/src/0_graph.rs`, riding the `GraphCx`/`Grade` plumbing `graph-callers-arm` lands. Wiring in `crates/sprefa-extract/src/bin/ryi.rs` is one clap flag pair (`--from`, `--expand`) added to the same `Some("graph")` dispatch arm the callers card writes; no new dispatch block. No new record types: `graph_node`, `graph_edge`, `graph_root` already exist from the parent card, this card only populates `graph_root.found = false` on a missing definition and adds the `--expand` fixpoint loop and the `next` block HATEOAS output (item.md lines 61-84) when the reachable set leaves an import unresolved into the traversal. Files touched: `src/0_graph.rs`, `src/bin/ryi.rs`. No touches to `src/types.rs` or `schema/1_facts.tsp`.

## Signatures

```rust
// crates/sprefa-extract/src/0_graph.rs

pub struct PathName { pub path: String, pub name: Option<String> } // parses "PATH:NAME"

impl std::str::FromStr for PathName {
    type Err = String;
    fn from_str(raw: &str) -> Result<PathName, String>; // splits on the last ':', name absent if none
}

pub struct FromOptions {
    pub expand: bool,
    pub kind: Option<String>,  // --kind call|type|both, default both
    pub budget: usize,         // ceil(bytes / 4); --budget, EXTRACT_BUDGET, extract.config.json, default
}

pub fn run_from(cx: &mut GraphCx, root: &PathName, opts: &FromOptions) -> i32; // process exit code, always 0

// pseudo-code
fn run_from(cx, root, opts):
    let Some(root_site) = cx.find_definition(&root.path, root.name.as_deref()) else {
        emit(FlatFact::GraphRoot { path: root.path.clone(), name: root.name.clone(), span: None, found: false })
        return 0   // AC: missing definition is not a refusal, exit 0
    }
    emit(FlatFact::GraphRoot { path: root.path.clone(), name: root.name.clone(), span: Some(root_site.span), found: true })

    let mut worklist = std::collections::VecDeque::from([(root.path.clone(), root.name.clone(), 0u32)])
    let mut left_set = false   // an import target outside the loaded path set was seen
    let mut split = GradeSplit::default()

    loop:
        while let Some((path, name, depth)) = worklist.pop_front():
            if !cx.visited.insert((path.clone(), name.clone())): continue
            let grade = Grade::from_origin(cx.origin_for(&path, &name))
            emit(FlatFact::GraphNode { path, name, depth, grade: grade.as_str().into(), line: cx.line_for(&path, depth) })
            for edge in cx.resolved_edges().filter(|e| e.caller_path == path && e.caller_name == name):
                let edge_grade = Grade::from_origin(&edge.resolution_origin)
                split.bump(edge_grade)
                emit(FlatFact::GraphEdge { from_path: path.clone(), from_name: name.clone(),
                    to_path: edge.callee_path.clone(), to_name: edge.callee_name.clone(),
                    kind: edge.kind.clone(), grade: edge_grade.as_str().into() })
                if !cx.loaded_paths.contains(&edge.callee_path):
                    left_set = true
                    if !opts.expand: continue   // do not chase outside the loaded file set
                worklist.push_back((edge.callee_path.clone(), edge.callee_name.clone(), depth + 1))

        if left_set && opts.expand && cx.load_more(opts.budget)?:  // step 0/1/2, item.md:80-84
            left_set = false
            continue  // re-run worklist over the freshly-loaded files; fixpoint when nothing new loads
        break

    emit_summary_line(&split)
    if left_set && !opts.expand:
        emit_next_block(&[
            format!("extract graph --from {}:{} --expand    close the universe, +N files",
                root.path, root.name.as_deref().unwrap_or("")),
            "extract x a                                    the unsupplied import targets",
            format!("extract graph --from {}:{} --grade +   only edges followed through a real import",
                root.path, root.name.as_deref().unwrap_or("")),
        ])
    0
```

## Fixture

Same fixture as `graph-callers-arm`: `tests/fixtures/ts5_findings/module_plane/`, same reason (closed under `resolved_import`, real call chains, a re-export cycle to prove the fixpoint terminates rather than looping). `deps/` stays rejected for the same reason: no call edges to reach.

Command: `ryi graph --from tests/fixtures/ts5_findings/module_plane/two_hop_consumer.ts:reach --expand --sqlite /tmp/graph.db`

Recursive-CTE oracle over the sqlite export; the `graph_node` depth counts this arm prints must agree with this query's `depth` groups:

```sql
WITH RECURSIVE reach(path, name, depth) AS (
  SELECT 'tests/fixtures/ts5_findings/module_plane/two_hop_consumer.ts', 'reach', 0
  UNION
  SELECT re.callee_path, re.callee_name, reach.depth + 1
  FROM resolved_edge re
  JOIN reach ON re.caller_path = reach.path
    AND (re.caller_name = reach.name OR (re.caller_name IS NULL AND reach.name IS NULL))
)
SELECT depth, count(*) FROM reach GROUP BY depth ORDER BY depth;
```

## Acceptance Criteria
- [ ] `extract graph --from src/main.ts:main --expand` reaches a fixpoint and reports which files it pulled in
- [ ] no input path exits with a refusal; an unclosed universe prints a `next` block naming `--expand`
- [ ] `--from` naming a missing definition emits `graph_root found=false`, exit 0
- [ ] depth counts agree with the recursive-CTE oracle over the same fixture
- [ ] `cargo test --features cli --no-fail-fast` green from `crates/sprefa-extract`

## Tests Run

## Implementation Notes

## Comments

## Decisions
