---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# soopy worktree enumerate walks the repo root and hashes every match for a subdirectory input

## Description

## Description
`ryi` directory and glob inputs expand through `soopy::SourceTree::enumerate` (crates/sprefa-extract/src/bin/ryi/1_inputs.rs). For a repository, soopy's worktree enumerate (`crates/soopy/src/_4_worktree.rs:27`) starts `WalkBuilder` at the repository root whatever the pattern prefix, and reads + blake3-hashes every matched file. Expanding `soopy/src` inside hafley-rs walks the whole monorepo: `ryi query ... soopy/src` takes 0.18s wall vs 0.02s for one explicit file. The hash is thrown away by the caller, which reads the bytes again.

## Acceptance Criteria
- [ ] enumerate starts the walk at the longest literal directory prefix of the patterns
- [ ] a caller that does not need content ids can skip the per-file hash
- [ ] timing receipt for `ryi query ... soopy/src` in hafley-rs

## Tests Run

## Implementation Notes
Found by fork inputs-cli (branch ryi/inputs-cli). Out of that fork's scope: soopy is shared.
