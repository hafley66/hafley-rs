---
created: 2026-09-20
updated: 2026-09-26
type: epic
owner: hafley66
status: obsolete
priority: normal
labels: [extract, artifact-cli]
---

# ryi new CLI verbs: graph, stratify, one-shot rename

## Description

Parent for every new ryi subcommand or new CLI surface. A card lands under this epic when it adds a verb or a flag family users invoke, so CLI implications are reviewed in one place. Children ordered: extract-graph-verb (three arms: --callers, --from, --uses) then ryi-stratify (blocked on graph) then rename-one-shot-planes (blocked on rename-path-double-reach, move-commit-exits-two).

## Repro receipt

2026-09-26: `ryii graph --uses Widget tests/fixtures/graph_rust/0_widget.rs tests/fixtures/graph_rust/1_reader.rs` exits 0 with 8 edges; graph work is already shipped.
