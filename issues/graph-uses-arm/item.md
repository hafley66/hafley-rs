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

Adds the `--uses TYPE` arm to `crates/sprefa-extract/src/0_graph.rs`, riding the `GraphCx`/`Grade` plumbing `graph-callers-arm` lands. Wiring in `crates/sprefa-extract/src/bin/ryi.rs` is one clap flag added to the same `Some("graph")` dispatch arm; no new dispatch block. No new record types: this arm emits the existing `graph_edge` shape from `graph-callers-arm`, sourced from `resolved_type_edge` instead of `resolved_edge`, with `kind` carrying the type-edge kind (`param`/`returns`/`uses`/`field`, `TypeEdgeKind::as_str` in `src/types.rs:268-278`) rather than `"call"`. Files touched: `src/0_graph.rs`, `src/bin/ryi.rs`. No touches to `src/types.rs` or `schema/1_facts.tsp`.

## Signatures

```rust
// crates/sprefa-extract/src/0_graph.rs

pub fn run_uses(cx: &GraphCx, type_name: &str) -> Vec<FlatFact>; // graph_edge rows over resolved_type_edge

// pseudo-code
fn run_uses(cx, type_name):
    let mut edges = Vec::new()
    let mut split = GradeSplit::default()
    for edge in cx.resolved_type_edges():   // filters FlatFact::ResolvedTypeEdge
        if edge.target_name.as_deref() == Some(type_name):
            let grade = Grade::from_origin(&edge.resolution_origin)
            split.bump(grade)
            edges.push(FlatFact::GraphEdge {
                from_path: edge.owner_path.clone(),  from_name: edge.owner_name.clone(),
                to_path: edge.target_path.clone(),   to_name: edge.target_name.clone(),
                kind: edge.kind.clone(),   // "param" | "returns" | "uses" | "field"
                grade: grade.as_str().into(),
            })
    emit_each(&edges)
    emit_summary_line(&split)
    edges
```

## Fixture

`tests/fixtures/ts_checker/src/` (not `module_plane`, `graph-callers-arm`'s and `graph-from-arm`'s fixture: `module_plane` has zero type annotations across files, so it produces zero `resolved_type_edge` rows). `main.ts:drive(widgets: Widget[])` is a `param` edge to `widget.ts:Widget`; `main.ts:seat(panels: Panel[])` is a `param` edge to `panel.ts:Panel`; both classes declare a `render()` method the golden can also assert on the `field`/`uses` side if the fixture is extended, but `param` alone already covers two of the four kinds cross-file. All five `.ts` files import only relative siblings (`./panel`, `./pick`, `./widget`), so the directory is closed under `resolved_import` on the same test as the other two cards.

`tests/fixtures/deps` stays rejected: it carries only `const` value imports, no `interface`/`class` type usage, so it cannot exercise `resolved_type_edge` either.

Command: `ryi graph --uses Widget tests/fixtures/ts_checker/src`

## Acceptance Criteria
- [ ] `extract graph --uses TYPE` returns the `param`/`returns`/`uses`/`field` rows for that type
- [ ] `cargo test --features cli` green
- [ ] `cargo test --features cli --no-fail-fast` green from `crates/sprefa-extract`

## Tests Run

## Implementation Notes

## Comments

## Decisions
