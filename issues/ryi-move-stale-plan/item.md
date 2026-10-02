---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: normal
labels: [ryi]
---

# ryi move --commit replays a stale dry-run plan

## Description

2026-10-02 hermetic branch: dry-run with --relocate-mod, then rename, then move --commit (no --relocate-mod) with the same --state applied the earlier plan: inserted '#[path = "_1_trace_projection.rs"] mod trace_projection;' mid-identifier at crates/boop-store/src/lib.rs:40 ('pub mod touche#[path...]...d;'), left 'n;' in ident.rs, rewrote 'use super::*' to 'use crate::ident::*' and 'pub(super)' to 'pub(crate::ident)' (invalid). Repaired by hand in 2d666ded.

## Comments

### 2026-10-02T14:47:38Z · @feature-ryi-ts-refactor

Commits: 20e3476b. Gate: UNVERIFIED, release build and crate tests terminated on the user's stop request; ryii, tsc, and dogfood cases unrun. Code and case scripts committed; coordinator runs gates serially. REPORT.md lists before/after behavior and the deferred invocation.
