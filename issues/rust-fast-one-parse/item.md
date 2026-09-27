---
created: 2026-09-25
updated: 2026-09-27
type: improvement
status: open
priority: normal
related: ['@rust-fast-macro-incremental']
labels: [extract]
---

# rust fast: one parse per file (drop syn from the fast path)

## Description

## Description

`RustSource::extract` parses each file with three parsers: tree-sitter (cst walk, scm query, call defs), syn (types, call sites, df, module facts via hafley_scm `lang::rust::*`), and ra_ap_syntax again inside local macro expansion. Measured with `sample` over a release `ryi fast` of 3000 registry .rs files (1.33M lines), inclusive samples out of ~16.4k per thread-set:

| work | samples |
| --- | --- |
| tree-sitter parse (`ts_parser_parse`) | 5579 |
| syn parse (`parse_rust_syntax`) | 2312 |
| hafley_scm query run | 2309 |
| syn-based row producers (call_site_rows, call_metadata_rows, module_resolution_rows, project_types, type_entity_rows) | ~1800 |

The chrome trace for the same run: `parse:"tree-sitter"` 3.17s and `parse:"syn"` 2.13s of 17.2s summed extraction.

Removing syn from the fast path means porting the hafley_scm Rust producers from `syn::File` to tree-sitter/scm queries (one parse, one arena), which is the scm++ read-side direction.

## Acceptance Criteria
- [ ] fast's Rust extraction parses each file once
- [x] ratchet 170 and the fast/slow diff unchanged
- [ ] release numbers before/after on the 3000-file registry corpus

## Projection progress

- [x] TSI syntax rows project from the shared tree and match Syn on pinned Rust fixtures.
- [x] Dataflow rows project from the shared tree and match Syn on pinned Rust fixtures.
- [ ] Module facts, receivers, and type candidates still use the remaining Syn projections.
- [ ] Parse count is still 2 per file; remaining projections must reach 1.

## Repro receipt

2026-09-27 after TSI and dataflow tree projections: `DL_TRAIL=0 DL_TRACE_SUMMARY=1 ryii fast crates/sprefa-extract/tests/fixtures/graph_rust/1_reader.rs` reports `rust parse calls=2`; ratchet 170, extraction suite, e2e, and workspace suite pass.

Approved: port the syn-based Rust projections to the existing tree-sitter parse, with one parse per Rust file. Current repro: `DL_TRAIL=0 DL_TRACE_SUMMARY=1 ryii fast crates/sprefa-extract/tests/fixtures/graph_rust/1_reader.rs` reports `rust parse calls=2`.
