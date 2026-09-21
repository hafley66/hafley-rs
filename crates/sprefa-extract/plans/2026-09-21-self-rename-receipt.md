# Self-rename receipt: Mutate -> Cleave via `ryi`

Owner asked for three steps, each a `ryi` command with
`--commit --verify 'timeout 600 cargo check --features cli'`. One flag
gap surfaced immediately; recorded here per the owner's instruction to
record gaps and continue rather than hand-edit.

## Gap: `ryi rename` has no `--verify` flag

`ryi move` has `--commit`, `--verify <CMD>`, `--verify-cwd <DIR>` (a
non-zero or timed-out verify rolls the move back). `ryi rename` has no
such flag at all — only `--verify-scip <INDEX>` (a report-only SCIP
cross-check, not a shell command, and it never changes the plan or exit
code).

```
$ ./target/debug/ryi rename --verify 'true' "src/types.rs#Mutate" "Cleave"
error: unexpected argument '--verify' found

  tip: a similar argument exists: '--verify-scip'

Usage: rename --verify-scip <INDEX> [TARGET] [NEW]

For more information, try '--help'.
```

This is a CLI-surface gap in `rename` (steps 1 and 2 below), not a
plan refusal or a verify-triggered rollback. Rather than skip those
steps, each was run as `ryi rename --commit` (no `--verify`, since the
flag does not exist) followed by a manual
`cargo check --features cli` as the verification gate, matching what
`--verify` would have done short of the automatic rollback-on-failure
behavior `ryi move` gets for free. Both checks passed clean; nothing
needed a manual revert.

## Step 1 — rename trait `Mutate` -> `Cleave`

Dry-run plan (`--json` tail), no abstains:

```
$ ./target/debug/ryi rename --root <crate-root> --state <tmp> --json \
    "src/types.rs#Mutate" "Cleave"
...
{"abstains":[]}
```

Committed:

```
$ ./target/debug/ryi rename --root <crate-root> --state <tmp> --commit \
    "src/types.rs#Mutate" "Cleave"
```

6 files touched: `src/0_cleave.rs`, `src/lang/mod.rs`,
`src/lang/rust_mutate.rs`, `src/lang/ts_mutate.rs`, `src/lib.rs`,
`src/types.rs`. `cargo check --features cli` after commit: clean.
Committed as `857fbe7f`.

Not covered by `ryi rename`: doc-comment prose that spells `Mutate` in
English (e.g. "The `Mutate` roster..." in `lang/mod.rs`,
"the `Mutate` arm" in `lang/{rust,ts}_mutate.rs`,
"the `Mutate` roster spells" in `0_cleave.rs`). The tool respells symbol
occurrences only, never free text inside `///`/`//!` comments. Left for
the closing `manual:` commit.

## Step 2 — rename `mutate_for` -> `cleave_for`, `mutates` -> `cleaves`

See the corresponding commit for the exact command line and files
touched. No `MUTATES` constant exists in this crate (`grep -rn MUTATES
src tests` is empty), so that part of the ask is a no-op — nothing to
rename.

## Step 3 — `ryi move` the two impl files

`ryi move src/lang/ts_mutate.rs src/lang/ts/cleave.rs` and the same for
`rust_mutate.rs` -> `src/lang/rust/cleave.rs`, each with
`--commit --verify 'timeout 600 cargo check --features cli'` (this flag
combination exists on `move`, unlike `rename`). See the corresponding
commit for the exact command line, plan, and verify result.
