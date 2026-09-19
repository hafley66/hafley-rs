---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: normal
epic: extract-parity-move-rename
related: ['@move-commit-exits-two']
labels: [extract, artifact-cli, phase-refinement-1, intent-correctness, component-rename, needs-chris]
---

# rename exits 6 with no plan when #[path] double-reaches a symbol

## Description

## Description

`rename` refuses with exit 6 and emits no plan when a `#[path]` attribute gives
two compilation routes to the target symbol. The crate's own tree triggers it, so
`rename` cannot be run on `sprefa-extract`.

```
rename "src/types.rs#ExtractOutput" RyiOutput --root <crate> --state <state> --text-refs
exit=6
stdout: (0 lines)
stderr: src/bin/extract.rs byte 1905: path attr twice reaches the symbol at runtime
        tests/0_sqlite.rs byte 56: path attr twice reaches the symbol at runtime
```

The two cited sites, with the byte offsets resolved by hand:

| file:line | attribute |
| --- | --- |
| `src/bin/extract.rs:40` | `#[path = "extract/0_sqlite.rs"]` |
| `tests/0_sqlite.rs:4` | `#[path = "../src/bin/extract/0_sqlite.rs"]` |

The shape is wider than those two lines. `src/bin/extract.rs` carries nine
`#[path]` attributes, six of them reaching `../` into the library's own sources:
`0_query.rs`, `0_move.rs`, `2_move_text.rs`, `3_region_writer.rs`, `4_watch.rs`,
`5_diff.rs`, `0_rename.rs`. Each of those files is compiled into both the lib
target and the bin target, so every symbol they declare is reachable twice.

`ExtractOutput` has 170 occurrences and `ExtractLang` 107. Both went unrenamed
in the 2026-09-18 pass because of this exit.

Open question for triage: whether the two routes are a genuine ambiguity that a
rename must refuse, or one symbol whose occurrence set is the union of both
routes. If the latter, exit 6 should be a plan.

## Acceptance Criteria

- [ ] The diagnostic prints `file:line`, not a byte offset.
- [ ] The diagnostic names both routes reaching the symbol, not just the attribute site.
- [ ] A decision is recorded: refuse by design, or plan over the union of routes.
- [ ] If planning: `rename src/types.rs#ExtractOutput RyiOutput` produces a plan covering all 170 occurrences across both targets.
- [ ] If refusing: the message states which flag or tree change unblocks the rename.
- [ ] A test fixture reproduces a two-route symbol and pins the chosen behavior.
