---
created: 2026-09-18
updated: 2026-09-18
type: task
status: needs-decision
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: dl8-rules
---

# dl8: port the corpus_unique leg as a .dl7 rule and diff against the Rust arm

## Description


Per AGENTS.md division of labor: the DL7 rule set is the spec of Resolve<CallF>. Port the simplest leg, corpus_unique, over `extract fast` JSONL facts and diff its resolved rows against the Rust arm's corpus_unique rows on the 18-file CTF corpus. Lives in ~/projects/sprefa (dl8), not this crate. Needs the user's go.

## Acceptance Criteria
- [ ] .dl7 program in dl8 producing resolved_edge rows with origin corpus_unique
- [ ] zero-diff against the Rust arm on the CTF corpus, or the diff filed as issues

## Plan

After authorization for the `~/projects/sprefa` lane, add a DL7 `corpus_unique` rule over the existing fast JSONL facts, run it and the Rust resolver on the same 18-file CTF snapshot, compare normalized edge keys and origins, then either record a zero-diff receipt or file each discrepancy with its minimal reproducer and rule boundary.

## Decision

Authorize the cross-repository DL7 implementation and comparison in `~/projects/sprefa`?
