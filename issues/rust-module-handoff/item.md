---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: fixed
priority: normal
closed: 2026-09-23
commits:
- hash: 7ef1984d
  summary: carry-module-facts
- hash: 6bcb4f88
  summary: verify-resolve-handoff
---

# Rust module facts handoff is never populated

_Source: crates/sprefa-extract/src/lang/rust_modules.rs_

## Description

rust_module_facts() first calls take_rust_module_facts(), but rust_stash_module_facts() has no caller. Resolve therefore falls back to syn::parse_file on files already parsed in the Rust extract arm. Wire a durable single-run handoff or derive the module rows from phase-1 facts, then assert one syn parse per Rust blob with --resolve.

## Resolution

### 2026-09-24T02:00:56Z · @issuectl

Rust extraction now carries module rows from its syn parse into project resolve; the regression test proves resolve uses that output without reparsing source bytes.
