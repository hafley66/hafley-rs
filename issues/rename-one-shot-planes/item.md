---
created: 2026-09-18
updated: 2026-09-20
type: task
status: open
priority: normal
epic: ryi-new-verbs
related: ['@move-commit-exits-two', '@rename-path-double-reach', '@extract-lines-flag']
labels: [extract, artifact-cli, intent-architecture, component-rename, phase-refinement-1, needs-chris]
---

# ryi rename in one shot: the three planes move and rename cannot reach

## Description

## Description

Renaming the `extract` binary to `ryi` on 2026-09-18 took 8 manual steps after
`move` ran. Receipts in `crates/sprefa-extract/docs/1_ryi-rename-by-its-own-verbs-20260918.md`.
Four of those steps are filed bugs. Three are capabilities neither verb has.

The target is one invocation per list, exit 0, gate green, zero hand edits:

```
ryi rename --list brand-symbols.tsv --commit --verify "cargo test --features cli"
ryi move   --list brand-paths.tsv   --commit --verify "cargo test --features cli"
```

### Plane 1: manifest target names

`move` rewrote `Cargo.toml` `bin[0].path` and left `bin[0].name` alone. A
`[[bin]]` target name is neither a filesystem path nor a bound symbol, so it sits
outside both verbs. Renaming it by hand is one line, and every consequence below
follows from it.

### Plane 2: cargo-derived identifiers

`env!("CARGO_BIN_EXE_extract")` appears 183 times across 129 test files. Cargo
synthesizes that identifier from the `[[bin]]` name; it is declared in no source
file, so no symbol plane sees it. The derivation rule is mechanical: bin name
`ryi` yields `CARGO_BIN_EXE_ryi`.

Other cargo-derived spellings in the same family, unaudited: `CARGO_BIN_NAME`,
`CARGO_CRATE_NAME`, `--bin <name>` in argv, and the binary's own filename under
`target/debug/`.

### Plane 3: a crate name inside a string literal

This is the one that shipped broken and compiled clean.
`tests/23_flow_cli_dispatch.rs:105` read `RUST_LOG=extract=info`. A `tracing`
target is the crate name, and for a bin target that is the `[[bin]]` name. After
the rename the filter matched nothing, the bench event never reached stderr, and
the failure surfaced as:

```
no bench event for ["--bench", "--family", "cfg", "tests/fixtures/resolve/0_caller.ts"]
```

Found by `grep -rn 'extract='` after the test failed. One site in this crate.
A rename that silences telemetry without a compile error is the worst available
failure mode, because a repo with no test on the log event would have shipped it.

### The input neither verb can derive

Brand versus English verb is a judgment call. 29 declarations in the crate carry
the word; the binary itself found them:

```
ryi --family cst <FILE>
  | grep '"kind":"(struct_item|enum_item|trait_item|type_item|function_item|mod_item|const_item|static_item)"'
  | grep -i '"name":"[A-Za-z_]*extract'
```

| class | members | count |
| --- | --- | --- |
| brand | `ExtractOutput`, `ExtractLang`, the `[[bin]]` target | 3 |
| English verb | `Source::extract` across 11 front-ends, `extract_file`, `extract_to`, `extract_pool`, `EXTRACT_POOL`, `extract_thread_cap`, `extract_observations`, `extracting_blob`, `ExtractingGuard`, `get_or_extract`, `EXTRACTIONS` | 26 |

The roster is written once, by hand, and becomes the `--list` input.

### Still spelled Extract on feat/ryi-rename

`rename` performed zero renames. It exits 6 on this crate for every target
because `#[path]` gives two compilation routes to the symbol, see
@rename-path-double-reach.

| symbol | occurrences | state |
| --- | --- | --- |
| `ExtractOutput` | 170 | unrenamed |
| `ExtractLang` | 107 | unrenamed |

## Acceptance Criteria

- [ ] @rename-path-double-reach is resolved, so `rename` produces a plan on this crate.
- [ ] @move-commit-exits-two is resolved, so a wrapper can trust the exit code.
- [ ] `move` or `rename` rewrites a `[[bin]]` target name, not only its path.
- [ ] Renaming a `[[bin]]` target rewrites every `CARGO_BIN_EXE_<name>` occurrence.
- [ ] Renaming a `[[bin]]` target reports or rewrites crate-name spellings inside string literals, `RUST_LOG` filters included.
- [ ] `CARGO_BIN_NAME`, `CARGO_CRATE_NAME` and `--bin <name>` are audited and either handled or documented as out of scope.
- [ ] `ExtractOutput` -> `RyiOutput` and `ExtractLang` -> `RyiLang` land in one `rename --list --commit` run.
- [ ] A test renames a bin target in a fixture crate and asserts the gate passes with zero hand edits.

## Tests Run

- [ ] `cargo test --features cli --no-fail-fast`

## Implementation Notes
