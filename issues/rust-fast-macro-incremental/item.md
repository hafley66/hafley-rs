---
created: 2026-09-25
updated: 2026-09-26
type: improvement
status: needs-decision
priority: normal
labels: [extract]
---

# hafley_scm macro expansion: incremental passes, one file sets the makespan

## Description

## Description

hafley_scm `expand_file` (local `macro_rules!` expansion) re-parses the whole file with ra_ap_syntax on every fixpoint pass (up to 8) and re-expands every surviving invocation. Failed expansions are now memoized (commit on ryi/fast-throughput: downcast-rs lib.rs 5.5s -> 0.72s), but successful tt-muncher chains remain costly:

- release, 3000 registry .rs files: `splice_macro_expansions` 4462 inclusive samples (`expand_file` 3170) of ~16.4k; `family:"call"` 7.9s of 17.2s summed extraction
- one file sets the extraction makespan: crossterm-0.29.0/src/style/stylize.rs 1.66s alone (wall of the whole parallel stage is 2.06s)

Directions: expand only the invocation subtrees a pass changed instead of reparsing the file; cap expansion work per file with the existing `budget_hit` flag; share one ra_ap parse with the rest of the Rust front-end.

## Acceptance Criteria
- [ ] no single registry file over 200ms in `family:"call"` (release)
- [ ] output identical on the macro-heavy registry set (downcast-rs, crossterm stylize, bitflags 1.3.2, castaway, borsh schema, byteorder, clap_builder debug_asserts)

## Repro receipt

2026-09-26: current `ryii fast` on crossterm 0.29 `style/stylize.rs` (6,933 bytes) takes 1.56s in `family:"call"` and reports 42 resolve calls.

Decision: prioritize incremental expansion of changed invocations, or cap per-file expansion work with the existing budget flag?
