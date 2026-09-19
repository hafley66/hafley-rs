---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract, artifact-cli, phase-refinement-1, intent-correctness, component-move]
---

# move --commit applies every move then exits 2

## Description

## Description

`move --list <tsv> --commit --verify <cmd>` applies every move and every
specifier repair, then exits 2 with a stderr line naming a source path it just
moved away.

Observed 2026-09-18 on branch `feat/ryi-rename` off `39189edb`, renaming the
`extract` bin to `ryi`:

```
move --list moves.tsv --root <crate> --state <state> --commit --verify "cargo test --features cli"
exit=2
stderr: move source is not a file: <crate>/src/bin/extract.rs
```

The tree after that run was correct and complete:

| site | state after exit 2 |
| --- | --- |
| `src/bin/ryi.rs`, `src/bin/ryi/help.rs`, `src/bin/ryi/0_sqlite.rs` | moved |
| `Cargo.toml` `bin[0].path` | `src/bin/ryi.rs` |
| `src/bin/ryi.rs:37,40` `#[path]` | repaired |
| `tests/0_sqlite.rs:4` `#[path]` | repaired |

The dry run of the identical list exited 0 and printed a 70-line plan.

Two consequences beyond the exit code. `--verify` never ran, so the rollback
guarantee the flag advertises was not exercised. And stdout was empty on the
`--commit` path while the dry run printed the full plan, so a caller that logs
the plan gets nothing on the run that actually changed the tree.

## Acceptance Criteria

- [ ] A multi-row `move --list --commit` exits 0 when every move applied.
- [ ] A later stage resolves source paths against the pre-move tree, or is ordered before the moves.
- [ ] `--commit` prints the same plan the dry run prints.
- [ ] `--verify` runs on the multi-row `--list` path, and a non-zero verify rolls every touched path back.
- [ ] A test covers `--list` with 3+ rows plus `--commit`, asserting exit 0 and the plan on stdout.
