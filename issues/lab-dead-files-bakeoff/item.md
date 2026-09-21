---
created: 2026-09-21
updated: 2026-09-21
type: improvement
status: open
priority: normal
related: ['@graph-callers-arm', '@lab-scopegraph-queries']
labels: [extract, lab]
---

# lab: dead files and file-graph bakeoff vs madge, knip, cargo dead_code

## Description

## Description

`ryi` already emits `file_edge` rows (crates/sprefa-extract, sqlite store). A dead file is a file with zero inbound `file_edge` that is not an entry point. Nobody has checked that view against the tools people already use. Lab crate `crates/lab-20260921-dead-files-bakeoff` (lab naming law: `lab-YYYYMMDD-<what>`), own workspace root, depends on sprefa-extract by path, zero edits under `crates/sprefa-extract/src`.

## Candidates

Table: tool / language / output / how compared.
- madge: ts, file import graph + orphans + cycles; `madge --json`, `madge --orphans`
- knip: ts, unused files, exports, dependencies; `knip --reporter json`
- rustc `dead_code` + `cargo check --message-format json`: rust, unused items (not files)
- cargo-machete: rust, unused crate deps only, listed for contrast
- rust orphan file: file under src/ with no `mod` line reaching it; the lab computes this itself from `file_edge`

## Steps

L1 sqlite view `dead_file` over `file_edge` (entry points: package.json main/exports/bin, `src/main.rs`, `src/lib.rs`, `tests/*.rs`, `benches/*.rs`). L2 ts judge on `crates/sprefa-extract/tests/fixtures/ts5_findings` and one real repo (this repo's `crates/boop-turnvis` has a deleted `1_claude_summary.rs`; pick a ts project the user has locally, ask via Boop-Ask if none) vs madge and knip, three-way split both/ryi-only/tool-only. L3 rust judge on `crates/sprefa-extract` vs `mod` reachability and rustc dead_code warnings. L4 REPORT.md with one table tool/language/agree/ryi-only/tool-only and one verdict sentence per language.

## Acceptance

Every disagreement listed with cause. Every command under `timeout 10`, every ryi run under `HAFLEY_TRACE`. Verdict sentence only, no recommendation.
