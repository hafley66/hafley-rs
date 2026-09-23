---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: open
priority: high
related: ['@item-move-import-closure', '@cleave-local-bindings']
---

# ryi cleave misses same-file callers and glob imports

## Description

Repro: from crates/sprefa-extract, run target/debug/ryi cleave src/lang/rust/1_type.rs#type_probe_key src/lang/rust_type_refs.rs --root . --state /private/tmp/ryi-scm-cleave-state --json. Dry-run only. Actual: callers is empty although resolve_type_dst in the source calls type_probe_key; the destination gains the function without its TypeEdgeKind dependency; use super::* is reported as orphan super. The preview would leave an unresolved source call and destination type. Expected: resolve same-file references to the moved item, add the destination import or qualified call, carry names supplied by a glob or refuse with a precise unresolved-name receipt, and never classify a glob as an ordinary one-name orphan. Add a focused typechecked cleave fixture.
