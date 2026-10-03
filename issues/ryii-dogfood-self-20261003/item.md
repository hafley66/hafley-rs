---
created: 2026-10-03
updated: 2026-10-03
type: bug
status: open
priority: normal
labels: [ryi]
---

# ryii dogfood on its own code (2026-10-03): fast graph misses #[path] bin calls and receiver-position calls, rename misses a test include, --uses misses turbofish types

## Description

Release `ryii` built from main `f5214b48` (`--features cli,ts-checker,typespec`), run
from `crates/sprefa-extract` over `src` (and `../hafley_scm/src`), checked against
ripgrep.

## Acceptance Criteria

- [ ] **Fast graph misses `crate::` calls between modules #[path]-included into a bin root.**
  `src/bin/ryi.rs:77-80` declares `#[path = "ryi/2_scmpp.rs"] mod scmpp;` and
  `#[path = "../0_query.rs"] mod query;`. `src/0_query.rs:196` calls
  `crate::scmpp::write_file(..)`. `ryii graph --callers write_file src` -> `0 edges`.
  `--slow` finds it (`src/0_query.rs:196 -> src/bin/ryi/2_scmpp.rs:179`, 6.3 s).
  `graph --from 0_query.rs#run` stops at `run_scmpp` and never reaches `write_file`,
  `check_sql`, `run_sql`. `ryii rename` on the same file does resolve the call, so fast
  graph and fast rename disagree on module resolution.
- [ ] **Fast graph misses a free-fn call in receiver position of a method chain.**
  `src/edit/_6_rename.rs:307` `let arm = rename_for(&request.anchor).ok_or_else(..)?;`.
  `ryii graph --callers rename_for src` lists only `_1_rename_cx.rs:21 owned_by`;
  `build_sequential:307` is missing. Sibling calls on lines 313-315 (`verify_spans(..)?`,
  `respells_for(..)?`) are found.
- [ ] **Fast rename misses a test that #[path]-includes the bin module.**
  `ryii rename src/bin/ryi/2_scmpp.rs#run_sql run_sql2 --root .` plans
  `src/0_query.rs 1`, `src/bin/ryi/2_scmpp.rs 1`, no abstains, exit 0.
  `tests/195_scmpp_growth.rs:87` calls `scmpp::run_sql(..)` through
  `#[path = "../src/bin/ryi/2_scmpp.rs"] mod scmpp;`; applying the plan breaks the test
  build. Expected: the site, or an abstain.
- [ ] **`graph --uses` misses a fully qualified type inside a turbofish.**
  `src/0_query.rs:156`
  `HashMap::<String, Option<hafley_scm::scmpp::Compiled>>::new()`.
  `ryii graph --uses Compiled crates/sprefa-extract/src crates/hafley_scm/src` lists
  `2_scmpp.rs` and `_2_compile.rs` users only; `run_scmpp` is missing.

## Tests Run

Commands above, release binary, 2026-10-03.

## Implementation Notes
