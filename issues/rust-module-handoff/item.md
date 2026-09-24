---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: open
priority: normal
---

# Rust module facts handoff is never populated

_Source: crates/sprefa-extract/src/lang/rust_modules.rs_

## Description

rust_module_facts() first calls take_rust_module_facts(), but rust_stash_module_facts() has no caller. Resolve therefore falls back to syn::parse_file on files already parsed in the Rust extract arm. Wire a durable single-run handoff or derive the module rows from phase-1 facts, then assert one syn parse per Rust blob with --resolve.
