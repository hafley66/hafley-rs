---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# Lab: stack-graph engine plus per-language tree-sitter query files, isolated crate

## Description

## Description

User 2026-09-18: "dont literally use those libs ... we need to lab before just lobbing onto our existing code." Audit (plans/reviews/2026-09-18-structure-audit-glm-REPORT.md) found 13 function names hand-copied across go/kotlin/rust/ts and 61% of src/ per-language.

Parrot three ideas, no library: (1) a per-language tree-sitter query file with fixed capture names (`@def.fn`, `@def.method @owner`, `@call @recv @name`, `@bind.name @bind.type`, `@import.path`) run through the crate's existing `query_source` / `TreeSitterQuery` (`src/lang/2_source_query.rs:25,95`); (2) a stack graph per blob (Root, Scope, Def, Ref, Push, Pop, Export, Import nodes), resolution as path search with a symbol stack, cross-file only through export nodes; spelled receivers as an edge from the binding's def into the type's member scope, so an untyped receiver has no path (the decline law); (3) origin column and unresolved rows as today.

Engine sketch:

```rust
enum NodeKind { Root, Scope, Def(Symbol), Ref(Symbol), Push(Symbol), Pop(Symbol), Export, Import(Path) }
struct Node { id: u32, kind: NodeKind, span: Option<Span>, blob: ContentId }
struct Graph { nodes: Vec<Node>, edges: Vec<(u32, u32)> }
struct Corpus { graphs: BTreeMap<ContentId, Graph>, exports: BTreeMap<String, Vec<(ContentId, u32)>> }
fn build(lang: &str, query: &str, src: &[u8], blob: ContentId) -> Graph;
fn resolve(corpus: &Corpus, blob: ContentId, r: u32) -> Outcome; // Def(blob, span) | Unresolved(reason)
```

Isolated crate `crates/sprefa-lab-scopegraph`; depends on sprefa-extract for parse and query only; zero edits under `crates/sprefa-extract/src`.

| step | content | done when |
| --- | --- | --- |
| L1 | crate skeleton | builds |
| L2 | kotlin `.scm`: fn/class/method defs, params, `val x: T`, calls, `x.m()`, imports, package | under 200 lines |
| L3 | engine per the signatures | under 800 lines |
| L4 | judge on `tests/fixtures/kotlin_receivers` + `kotlin_module_resolve` against `extract fast` (receiver 7, module_plane 11) and the unresolved set | zero disagreements or each explained |
| L5 | ts `.scm`, no engine change | engine diff 0 lines |
| L6 | scip ratchet over lab edges | `true` floors hold |

Stack graphs upstream is archived (github/stack-graphs, 2025-09-09); this is a from-scratch parrot, MIT-shaped ideas only.

## Acceptance Criteria
- [ ] L1-L4 on kotlin: zero unexplained disagreements with extract fast
- [ ] L5 on ts with zero engine edits
- [ ] a written verdict: scale or do not scale, with the .scm and engine line counts
