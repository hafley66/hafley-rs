---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: fixed
priority: high
related: ['@item-move-import-closure', '@cleave-local-bindings']
closed: 2026-09-23
commits:
- hash: a16077de
  summary: Kept same-file callers and expanded Rust parent globs
- hash: 877c3a11
  summary: Added passing cleave fixtures
---

# ryi cleave misses same-file callers and glob imports

## Description

Repro: from crates/sprefa-extract, run target/debug/ryi cleave src/lang/rust/1_type.rs#type_probe_key src/lang/rust_type_refs.rs --root . --state /private/tmp/ryi-scm-cleave-state --json. Dry-run only. Actual: callers is empty although resolve_type_dst in the source calls type_probe_key; the destination gains the function without its TypeEdgeKind dependency; use super::* is reported as orphan super. The preview would leave an unresolved source call and destination type. Expected: resolve same-file references to the moved item, add the destination import or qualified call, carry names supplied by a glob or refuse with a precise unresolved-name receipt, and never classify a glob as an ordinary one-name orphan. Add a focused typechecked cleave fixture.

## Resolution

### 2026-09-23T23:26:15Z · @issuectl

Same-file callers retain a source import; use super::* carries referenced parent imports and exported declarations or reports a private declaration. The type_probe_key move passed cargo check through cleave --commit --verify, and focused same-file/glob fixtures pass. Other glob forms remain outside this repro.
