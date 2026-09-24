---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: fixed
priority: high
related: ['@item-move-import-closure']
closed: 2026-09-23
commits:
- hash: a598e0f9
  summary: Moved Rust macro call projection with ryi cleave
- hash: 2ea77f17
  summary: Added executable numbered-module regression
---

# ryi cleave misses numbered module aliases and child import scope

## Description

Repro from crates/sprefa-extract: `ryi cleave src/lang/rust/lib.rs#splice_macro_expansions src/lang/rust/2_call.rs --root . --state /private/tmp/ryi-scm-cleave-state --commit --verify 'cargo check --features cli -q'`. The first preview emitted `use crate::lang::rust::2_call::splice_macro_expansions;` although `lib.rs` declares `#[path = "2_call.rs"] mod call_facts;`. After that spelling was corrected, verification found 85 compile errors: parent imports used by child `use super::*` modules were pruned, and the destination imported names it already declares. The verify journal rolled back both files. Expected: resolve the declared module alias, preserve imports visible to children, and treat destination declarations as bound names. Preflight commit `997a7e76` caught the invalid Rust before durable staging.

## Resolution

### 2026-09-24T00:32:23Z · @issuectl

Rust module paths now resolve #[path] aliases, parent imports remain when child modules can consume them, and destination declarations count as bound. The real splice_macro_expansions move passed cleave --commit --verify cargo check; the numbered-module regression and full crate gate passed.
