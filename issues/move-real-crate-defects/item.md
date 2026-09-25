---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# move on a real crate: path mods, own-lib ident, relative qualifiers, pub ancestors

## Description

## Description
Found by M6 while moving mutation files within sprefa-extract (lang/ -> edit/). Fixed in lang/rust_rehome.rs:
1. A `#[path]` mod decl was skipped by the relocate plan; it now keeps its declared name and is re-aimed.
2. A bin/test/example spelling its own lib by package ident was not rewritten; it is now treated like `crate::`.
3. A moving file's `super::`/`self::` qualifier broke when its new home no longer reached the target; it is now respelled from `crate`.
4. A `pub` module moved under a private ancestor became unreachable; ancestor mods on the new path are now written `pub`.
5. widen_privates widened every same-named item; only paths through the module (`.., name, item`) count now.

## Acceptance Criteria
- [x] fixes 1-5 in rust_rehome.rs
- [ ] a fixture test per fix
