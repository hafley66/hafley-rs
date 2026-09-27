---
created: 2026-09-25
updated: 2026-09-26
type: bug
status: fixed
priority: normal
labels: [extract]
---

# ryi prints one INFO extract_file line per input to stderr by default

## Description

## Description
Every multi-file `ryi` run prints one `INFO extract_file{path=...}` span line per input to stderr under the default `RUST_LOG` (sprefa_extract=info, src/trace.rs). `ryi query --entry soopy/src/_13_fetch.rs` printed ~60 such lines before its 3 result rows; six reader agents on 2026-09-25 each needed `2>/dev/null` for `ryi graph`.

## Acceptance Criteria
- [x] default level warn; `RUST_LOG=sprefa_extract=info` restores the spans
- [x] `ryi --help` text and tests/111 name the new default
- [x] a default `ryi graph` / `ryi fast` run writes nothing to stderr but the graph summary line and warnings

## Tests Run

`cargo nextest run --features cli -j 2 --test all -E 'test(/^t_(31_tracing|111_cli_identity|178_ryi_help)::/)'` passed (16 tests).

## Implementation Notes
Plan step 5 of plans/2026-09-25-ryi-cli-cleanup.md. The queue acceptance sets `warn` as the default; explicit `RUST_LOG=sprefa_extract=info,hafley_scm=info` retains per-file span timing output.

## Comments

### 2026-09-25T17:16:06Z · @claude-perf

Measured 2026-09-25 (release, ryi fast crates/sprefa-extract/src, 110 files): default observe layers 0.14-0.16s wall vs DL_TRAIL=0 RUST_LOG=off 0.13-0.14s; earlier baseline 0.90s vs 0.87s. The per-file INFO spans cost ~3-7% of wall; the decision stays with the user.
