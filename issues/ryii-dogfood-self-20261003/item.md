---
created: 2026-10-03
updated: 2026-10-03
type: bug
status: open
priority: normal
labels: [ryi]
---

# ryii dogfood on its own code (2026-10-03): fast graph misses #[path] bin calls and crate-name imports from own lib, rename misses a test include, --uses misses turbofish types

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
- [ ] **Fast graph does not bind calls imported from the crate's own lib by crate name.**
  `src/edit/_6_rename.rs:17` `use sprefa_extract::{.., rename_for, ..}` (the file is
  compiled into the `ryi` bin; `rename_for` is `src/edit.rs:97` in the lib crate).
  The call sites are extracted (`site` rows, callee `rename_for`, lines 202, 307, 472),
  but `ryii graph --callers rename_for src` lists only `_1_rename_cx.rs:21 owned_by`.
  Sibling calls to same-file fns (lines 313-315) bind. The chain shape
  `rename_for(..).ok_or_else(..)?` is not the cause.
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
