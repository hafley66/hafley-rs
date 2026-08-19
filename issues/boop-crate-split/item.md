---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: boop-process
size: L
blocked_by: ['@boop-main-split']
---

# Split boop into boop-store / boop-harness / boop-mail / boop-proc / boop-cli

## Description

## Description
Split `crates/boop` (33363 lines) into `boop-store`, `boop-harness`, `boop-mail`, `boop-proc`, `boop-cli` per `docs/design/boop-process.md` section 3. One PR per crate extraction in dependency order: store, harness, mail, proc, cli. Zero behavior change; `boop --help` per verb byte-identical before and after (pinned test); `cargo test` wall time unchanged or better.
## Acceptance Criteria
- [ ] workspace has the five crates; `crates/boop` is gone or is the bin crate only.
- [ ] no crate runs SQL against another crate's tables by string; `boop-store` exposes typed fns; `boop-proc` does not depend on clap.
- [ ] `test_support` becomes `boop-store`'s `testing` feature; every integration test moves with its crate; `tests/temp_home_rail.rs` still covers all of them.
- [ ] `cargo-semver-checks` on CI covers each new lib crate.
- [ ] `docs/design/boop-process.md` section 3 table updated to the real file list after the move.
