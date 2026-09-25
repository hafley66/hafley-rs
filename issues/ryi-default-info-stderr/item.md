---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# ryi prints one INFO extract_file line per input to stderr by default

## Description

## Description
Every multi-file `ryi` run prints one `INFO extract_file{path=...}` span line per input to stderr under the default `RUST_LOG` (sprefa_extract=info, src/trace.rs). `ryi query --entry soopy/src/_13_fetch.rs` printed ~60 such lines before its 3 result rows; six reader agents on 2026-09-25 each needed `2>/dev/null` for `ryi graph`.

## Acceptance Criteria
- [ ] default level warn; `RUST_LOG=sprefa_extract=info` restores the spans
- [ ] `ryi --help` text and tests/111 name the new default
- [ ] a default `ryi graph` / `ryi fast` run writes nothing to stderr but the graph summary line and warnings

## Tests Run

## Implementation Notes
Plan step 5 of plans/2026-09-25-ryi-cli-cleanup.md. Conflict to resolve with the user first: src/trace.rs:579 records the info default as user-set 2026-09-18 ("one line per file with its phase timings"), and tests/31_tracing.rs `the_info_default_narrates_an_ordinary_run_on_stderr` pins it.
