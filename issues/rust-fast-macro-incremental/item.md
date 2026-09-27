---
created: 2026-09-25
updated: 2026-09-27
type: improvement
status: open
priority: normal
labels: [extract]
---

# hafley_scm macro expansion: incremental passes, one file sets the makespan

## Description

hafley_scm `expand_file` (local `macro_rules!` expansion) re-parses the whole file with ra_ap_syntax on every fixpoint pass (up to 8) and re-expands every surviving invocation. Failed expansions are now memoized (commit on ryi/fast-throughput: downcast-rs lib.rs 5.5s -> 0.72s), but successful tt-muncher chains remain costly:

- release, 3000 registry .rs files: `splice_macro_expansions` 4462 inclusive samples (`expand_file` 3170) of ~16.4k; `family:"call"` 7.9s of 17.2s summed extraction
- one file sets the extraction makespan: crossterm-0.29.0/src/style/stylize.rs 1.66s alone (wall of the whole parallel stage is 2.06s)

Directions: expand only the invocation subtrees a pass changed instead of reparsing the file; cap expansion work per file with the existing `budget_hit` flag; share one ra_ap parse with the rest of the Rust front-end.

## Acceptance Criteria
- [ ] no single registry file over 200ms in `family:"call"` (release)
- [x] output identical on the macro-heavy registry set (downcast-rs, crossterm stylize, bitflags 1.3.2, castaway, borsh schema, byteorder, clap_builder debug_asserts)

## Repro receipt

2026-09-26: current `ryii fast` on crossterm 0.29 `style/stylize.rs` (6,933 bytes) takes 1.56s in `family:"call"` and reports 42 resolve calls.

## Work receipt

2026-09-27: incrementally scan only inserted macro subtrees after the first pass, reparse once per changed pass, and cache parsed macro definitions and exact successful expansions. Seven-file output matches `/tmp/fastmacro-baseline` byte for byte. Release `family:"call"` timings: downcast-rs 926.2ms, crossterm 79.8ms, bitflags 355.6ms, castaway 214.9ms, borsh 418.1ms, byteorder 167.6ms, clap_builder 100.7ms. Remaining performance gate is open; reproduce with `DL_TRACE_SUMMARY=1 ryii fast <registry-file>`.

Verification: `cargo nextest run --features cli -j 2 --offline --locked --test all` passed 1,118, 18 skipped; `scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/release` passed 14/14; `cargo nextest run --workspace -j 2 -E "not (test(/e2e|live|tmux|tui_sigint|omp_live/))"` passed 1,366, 203 skipped.
