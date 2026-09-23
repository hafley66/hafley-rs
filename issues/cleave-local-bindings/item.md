---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: open
priority: high
related: ['@item-move-import-closure']
---

# ryi cleave exports body-local bindings

## Description

Repro: from crates/sprefa-extract, run RUST_LOG=sprefa_extract=warn target/debug/ryi cleave src/lang/rust/lib.rs#project_df src/lang/rust/3_df.rs --root . --state /private/tmp/ryi-scm-cleave-state --drag --json. Dry-run only; no files were changed by cleave. Actual plan lists body locals start, end, index, item, sig, ident, inner, types, and tail as dragged/exported declarations. The preview inserts pub before let bindings inside functions, pub pub(crate) before def_span, and stray let fragments into the destination. The same false body-local exports appear without --drag. Expected: classify free names at item/module scope only; never export or move a local binding; preview parses before commit. Add a focused regression fixture, then typecheck the project_df cleave output.
