---
created: 2026-09-18
updated: 2026-09-18
type: improvement
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-cli
---

# CLI: teaching errors, extract why, witness rows

## Description


Brief ready: TASKS/lane-cli-teach.BRIEF.md. Known defects: `extract slow --project-root <crate> src/lib.rs` reports no_markers although Cargo.toml exists (scip_ensure.rs:830-837); `extract fast --witness` emits zero witness rows. Add `extract why <file:line>` printing the legs tried and the drop reason.

## Acceptance Criteria
- [ ] both defects have a failing-then-passing integration test through the binary
- [ ] `extract why` documented in the CLI help
