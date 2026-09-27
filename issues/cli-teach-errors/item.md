---
created: 2026-09-18
updated: 2026-09-26
type: improvement
status: obsolete
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-cli
closed: 2026-09-26
disposition_note: 'Current ryi rejects the retired extract verb (ryii extract fast: “extract does not exist”, exit 2); card defects do not reproduce on this surface.'
---

# CLI: teaching errors, extract why, witness rows

## Description


Brief ready: TASKS/lane-cli-teach.BRIEF.md. Known defects: `extract slow --project-root <crate> src/lib.rs` reports no_markers although Cargo.toml exists (scip_ensure.rs:830-837); `extract fast --witness` emits zero witness rows. Add `extract why <file:line>` printing the legs tried and the drop reason.

## Acceptance Criteria
- [ ] both defects have a failing-then-passing integration test through the binary
- [ ] `extract why` documented in the CLI help

## Comments

### 2026-09-27T01:58:59Z · @codex

Repro receipt: current ryi rejects the retired extract verb (ryii extract fast -> “extract does not exist”, exit 2); card defects do not reproduce on this CLI surface.

### 2026-09-27T01:59:02Z · @intake

Obsolete: Current ryi rejects the retired extract verb (ryii extract fast: “extract does not exist”, exit 2); card defects do not reproduce on this surface.
