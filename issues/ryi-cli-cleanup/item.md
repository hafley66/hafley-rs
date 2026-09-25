---
created: 2026-09-25
updated: 2026-09-25
type: improvement
status: open
priority: normal
epic: ryi-new-verbs
related: ['@cli-teach-errors', '@extract-lines-flag', '@extract-graph-verb']
labels: [extract, artifact-cli]
---

# ryi CLI cleanup: turnkey fast and slow over one input model

Plan: crates/sprefa-extract/plans/2026-09-25-ryi-cli-cleanup.md

## Description
SCIP and the checkers are the approving oracle; fast is what runs across hundreds of repos. Both must write the same projection tables so conformance is a SQL diff. Today `ryi slow ROOT` dumps raw scip_* rows that join to nothing fast writes; the SCIP -> (site span, def span) projection lives only in tests/fixtures/ratchet_soopy/regen.sh and the GRADE SQL of tests/170. `--resolve --project-root R` and `graph --project-root R` adopt a fresh cached index (project.rs:1623; `ryi fast` passes no root and stays syntax-only), and `--resolve --scip-index` mixes syntax legs and the scip leg into one table (soopy: same_file 443 + scip 135).

Input handling is also split four ways (files only, one dir, files+dirs without .gitignore, one file) with four hand walkers (0_graph.rs:292, 7_scm_rows.rs:334, move_cx.rs:45, rename_cx.rs:46) beside soopy's gitignore+globset enumeration; --family, --state, --json and the root dir each carry several meanings; the three --*-checker flags do nothing in the default build. Rust mod edges never reach resolved_import (19 of 29 soopy files reachable from lib.rs).

## Acceptance Criteria
- [ ] fast and slow share TierArgs (Inputs + --sqlite + --lines + --arms + --witness) and write the same table set
- [ ] slow_project: SCIP occurrences (find or build index, cached, one per language present, merged) projected onto resolved_edge / unresolved / resolved_import / resolved_type_edge / symbol / occurrence with origin scip; checkers into the same tables when compiled
- [ ] no syntax-tier verb loads or adopts an index
- [ ] raw scip_* dump moves to `ryi scip`
- [ ] Inputs (PATH|DIR|GLOB|-, --pattern, --entry, --depth, --root) on fast, slow, graph, query; expansion through soopy only; the four hand walkers deleted
- [ ] RustModuleIndex emits resolved_import kind module per mod decl; --entry lib.rs on the soopy fixture reaches all 29 files
- [ ] ratchet 170 diffs slow.sqlite against fast.sqlite; regen.sh reduces to `ryi slow`
- [ ] --ingest/--schema/--trail become verbs; --family split into --arms/--kinds; --root everywhere; graph --state -> --sqlite; watch --state -> --receipts; graph/diff --json removed
- [ ] query: --lang optional, many inputs, path column, --sqlite
- [ ] default log level warn
- [ ] tests/ migrated; cargo test --features cli green
- [ ] schema/3_cli.tsp deleted or regenerated

## Tests Run

## Implementation Notes
