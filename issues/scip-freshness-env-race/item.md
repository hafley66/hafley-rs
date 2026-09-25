---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
labels: [extract]
closed: 2026-09-25
---

# scip_freshness test races SPREFA_SCIP_INDEX env

## Description

## Description
`tests/scip_freshness.rs` `stale_set_rebuilds_and_the_original_set_still_hits` failed once under the full `cargo test --features cli` run (line 81, "the v5 form with no set asked is unchanged") and passes alone 4/4. `index_path_for_set` reads the process-global `SPREFA_SCIP_INDEX`, which sibling tests set under the `ENVIRONMENT` lock; this test reads it without the lock.

## Acceptance Criteria
- [ ] every test in the file that reaches `index_path_for_set` holds `ENVIRONMENT`
