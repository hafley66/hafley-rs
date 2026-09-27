---
created: 2026-09-21
updated: 2026-09-27
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

- [x] SQLite `dead_file` view and source entry-point exclusion implemented in `crates/lab-20260921-dead-files-bakeoff`.
- [x] madge fixture comparison lists agreement and both disagreement sets.
- [x] Compare the TypeScript fixture with Knip; list every disagreement and its cause.
- [ ] Repeat madge and Knip on a user-selected real TypeScript repository.
- [x] Compare fixture-sized Rust `mod` reachability and rustc `dead_code`.
- [x] Complete `REPORT.md` with locally runnable measurements and verdicts; only the selected-repository gate remains.

Open-gate commands are recorded in `crates/lab-20260921-dead-files-bakeoff/REPORT.md`.

## Reproduction receipt

2026-09-27 before implementation: `find crates -maxdepth 2 -type d -name 'lab-*'` returned no lab crate; `crates/lab-20260921-dead-files-bakeoff` and its `REPORT.md` were absent.

2026-09-27 current-ryii repro: `HAFLEY_TRACE="$HOME/.cache/lanes/the-gang-graph/dead-files/ryi-ts5-trace.json" timeout 10 "$HOME/.cache/boop/cargo-target/debug/ryii" --deps --root crates/sprefa-extract/tests/fixtures/ts5_findings crates/sprefa-extract/tests/fixtures/ts5_findings > "$HOME/.cache/lanes/the-gang-graph/dead-files/ryi-ts5-relative.jsonl"` completed and emitted 33 `file_edge` rows for the 61-file fixture. Madge and Ryi share 32 orphan paths. Knip and Rust fixture measurements are recorded below; only the user-selected repository remains open.

2026-09-27 measurement receipt: Knip was run with `npx -y knip` against a lane-scratch copy of `ts5_findings` with a minimal package manifest. Comparison: agreement 32, ryi-only 0, Knip-only 29; all Knip-only paths and the no-entrypoint cause are in `REPORT.md`. On the 3-source Rust fixture, `ryi --deps` emitted `file_unresolved` for `mod live;` (`node_modules_boundary`); module reachability found only `src/orphan.rs`, while rustc `dead_code` named an unused item in reachable `src/live.rs`. The lab CLI reported the file-path sets; its JSON-array handling and source-only unresolved-row decoding are covered by two new tests. `cargo nextest run --manifest-path crates/lab-20260921-dead-files-bakeoff/Cargo.toml -j 2 --locked --offline --test 0_dead_files` passed 5/5. The workspace gate passed 1378/1378 with 203 excluded and 1 leaky test. The user-selected real-repository gate is the only remaining measurement.

2026-09-27 Rust module resolver correction: the fixture now lives in `crates/sprefa-extract/tests/fixtures/rust_module_reachability`. `ryii --deps` emits `file_edge(src_path="src/lib.rs", dst_path="src/live.rs", kind="module", symbols=1)`. The lab comparison flipped from agreement 1 / Ryi-only 1 / tool-only 0 to agreement 1 / Ryi-only 0 / tool-only 0 against `mod` reachability. Against rustc `dead_code`, the file-path comparison is agreement 0 / Ryi-only 1 / tool-only 1 (`src/orphan.rs` vs `src/live.rs`).

2026-09-26: fixture comparison on `crates/sprefa-extract/tests/fixtures/ts5_findings`:

- `timeout 10 madge --json --extensions ts crates/sprefa-extract/tests/fixtures/ts5_findings`: 61 files, 32 edges.
- `timeout 10 madge --orphans --extensions ts crates/sprefa-extract/tests/fixtures/ts5_findings`: 32 orphans.
- `HAFLEY_TRACE=1 timeout 10 /Users/chrishafley/.cache/boop/cargo-target/debug/ryii --deps --root crates/sprefa-extract/tests/fixtures/ts5_findings crates/sprefa-extract/tests/fixtures/ts5_findings`: 33 `file_edge` rows; comparing destinations against the 61 fixture files gives 32 zero-inbound files.
- The two orphan sets were equal: 0 madge-only, 0 ryi-only. The lab CLI comparison reports 32 shared paths, 0 ryi-only, and 0 madge-only. Knip was unavailable at this point; the later `npx -y knip` measurement is recorded below.

2026-09-27: fixture e2e `HAFLEY_TRACE=1 timeout 10 .../ryii --deps ...`, `timeout 10 madge --orphans --extensions ts ...`, and the lab comparison report 33 edges, 32 shared orphan paths, and 0 disagreements. `cargo nextest run -j 2 --offline` in the lab crate passed 3 tests at that point. Subsequent fixture measurements and the updated 5-test receipt are recorded above; only the real-repository measurements remain open.
