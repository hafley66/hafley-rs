---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# cross-crate cleave: third-party deps unchecked, private dest module unreachable

## Description

## Description
`ryi cleave crates/sprefa-extract/src/types.rs#Span crates/hafley_scm/src/types/span.rs` planned `use serde::Serialize;` into hafley_scm, which did not depend on serde. The plan printed `travel Serialize from serde::Serialize as serde::Serialize (package)` and exited 0; the build would have failed. cross_package_stop only checked workspace-package edges.

Second finding, same run: the destination `hafley_scm/src/types/span.rs` sits under the private `mod types;`, and callers were spelled `hafley_scm::types::span::Span`, a path other crates cannot reach. Worked around by cleaving into `hafley_scm/src/span.rs` (a `pub mod` in lib.rs).

## Acceptance Criteria
- [x] a travelling third-party import missing from DEST's manifest is a named stop (exit 2): `cleave across packages: beta must depend on serde_json (tagged imports Value from serde_json::Value); add the dependency` (tests/173_move_cross_crate.rs)
- [ ] a cross-crate cleave into a private module either publishes the ancestors (as move does) or spells callers through a public re-export
