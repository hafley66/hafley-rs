# Brief: TypeScript checker tier on stock tsgo (TS 7.0.2), retire the Node script

Base: origin/main. Runs in parallel with fix/ryi-dogfood-defects (Rust module index + cleave/rename).
Do not touch: crates/hafley_scm/src/read/lang/rust_modules.rs, 0_rust_module_context.rs,
crates/sprefa-extract/src/edit/_7_cleave*.rs, 7*_cleave_*.rs, rename_cx. If a change there is
needed, stop and report.

## Why
TSI (`read/tsi/`, trait `SemanticRows`, `project.rs:296` `resolve_project_with_tsi_tiers`,
`CHECKER_TIERS` at `project.rs:941`) has a ts tier backed by `ts_checker.mjs` (TS5 JS API). It crashes
on 7.0.2 at `ts.sys`. TS7 has no JS API. Decision (user, 2026-10-05): no tsgo patch; stock only.

## Design (decided)
- One tsgo process per root: LSP without didOpen for definition/references (unopened files are
  read from disk), plus the API session hosted in the same process (`custom/initializeAPISession`).
- TSI relations produced from stock calls:
  | relation | stock source |
  |---|---|
  | identity (symbol at a position) | LSP definition / API symbol at position |
  | has_type | API getTypesAtPositions |
  | assignable | API per-pair assignability |
  | conforms | API getBaseTypes |
  | type structure | API per type handle |
- subtype / strict_subtype / identical / comparable: not produced; the coverage claim says so
  (`CoverageClaim { complete: false, diagnostic }`).
- Demand rule: ask only about sites the request names; open nothing that is not needed.
- Reuse the existing LSP session code (`crates/sprefa-extract/src/edit/1a_ts7_lsp_session.rs`);
  the tier lives in hafley_scm or calls it, one session implementation, not two.
- Lab evidence for the stock API path: `crates/sprefa-extract/bench/labs/lab-20261005-tsgo-wedge/`
  (`1_map.md`, `2_wedges/client.ts`, README Results: stock API 0.64 s on 133 files / 581 sites).

## Work, one commit each
1. Failing tests first: t_92 / t_98 / t_100 / t_101 / t_104 (ts checker + TSI) expect the stock
   coverage (no subtype/identical/comparable). Snapshot whole outputs.
2. tsgo `SemanticRows` implementation; register as the ts entry of CHECKER_TIERS.
3. Delete `ts_checker.mjs` and the Node driver code in `ts_checker.rs`. Move `DriverRequest` /
   `parse_driver_stdout` (used by the go tier) into `go_checker.rs`.
4. Same-root parity check vs the old tier on a small fixture where the old one still runs
   (TS 5.9 checkout ~/projects/TypeScript-5.9 if needed): table relation / rows_old / rows_new.

## Limits
- `npm ci` in crates/sprefa-extract/ts7 first. Build with KACHE_DISABLED=1; feature `cli,ts-checker`.
- Commands under 2 min; no whole-corpus runs; one run per measurement.
- Rust profile already set. Full suite once at the end in background; failures split base/new.
- No merge, no push.
