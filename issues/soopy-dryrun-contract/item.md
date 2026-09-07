---
created: 2026-09-07
updated: 2026-09-07
type: improvement
reporter: chrishafley
status: untriaged
priority: normal
labels: [domain-soopy]
provenance: codex
source_ref: sprefa:4847:soopy-dryrun-contract
---

# Make Soopy dry-run mutation behavior explicit

## Description

## Evidence
crates/soopy/README.md DryRun section documents that CommitEngine::open_dry_run still applies actions to its target while disabling device flushes. It requires a disposable mirror. The API name can be mistaken for a no-write preview.
## Acceptance Criteria
- [ ] Make the distinction between read-only planning, stage-store writes, and non-durable mirror application explicit in public API documentation and examples.
- [ ] Review naming or an explicit disposable-target guard before exposing this through CLI.
- [ ] Tests demonstrate which paths change in each mode and that preview does not alter target bytes.
## Scope
No claim that the current documented implementation is broken. Track API misuse risk; preserve the existing non-durable benchmark capability.
## Tests Run
README and source inspected; tests not executed.
