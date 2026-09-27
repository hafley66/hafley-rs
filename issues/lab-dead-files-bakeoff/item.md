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

## Reproduction receipt

2026-09-26: fixture comparison on `crates/sprefa-extract/tests/fixtures/ts5_findings`:

- `timeout 10 madge --json --extensions ts crates/sprefa-extract/tests/fixtures/ts5_findings`: 61 files, 32 edges.
- `timeout 10 madge --orphans --extensions ts crates/sprefa-extract/tests/fixtures/ts5_findings`: 32 orphans.
- `HAFLEY_TRACE=1 timeout 10 /Users/chrishafley/.cache/boop/cargo-target/debug/ryii --deps --root crates/sprefa-extract/tests/fixtures/ts5_findings crates/sprefa-extract/tests/fixtures/ts5_findings`: 33 `file_edge` rows; comparing destinations against the 61 fixture files gives 32 zero-inbound files.
- The two orphan sets were equal: 0 madge-only, 0 ryi-only. `knip` is unavailable (`command -v knip` returned no path).

Still open: run `timeout 10 knip --reporter json <typescript-repository>` and repeat the madge/ryii comparison on the real TypeScript repository the user selects. `crates/boop-turnvis` has no TypeScript source in this checkout. No `crates/lab-20260921-dead-files-bakeoff` implementation or three-way report exists yet.
