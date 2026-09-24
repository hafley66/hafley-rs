# Lane: flash-review cleanups, twelve rows

Repo: `~/projects/hafley-rs`. Crate: `crates/sprefa-extract`. Base: `75f41c7a`.
Issue: `issues/flash-review-cleanups/item.md`. Epic: `extract-parity-move-rename`.

ONE cargo at a time. Never run a second cargo while one is building.

Another lane owns `schema/1_facts.tsp`, `src/wire.rs`, `src/bin/ryi/0_sqlite.rs`
and `src/bin/ryi.rs` right now. Do not edit those four files. If a fix seems to
need one, stop and report it instead.

## Owned files

- `crates/sprefa-extract/tests/130_*.rs`
- `crates/sprefa-extract/tests/131_*.rs`
- `crates/sprefa-extract/tests/134_*.rs`
- `crates/sprefa-extract/tests/golden_parity.rs`
- `crates/sprefa-extract/src/lang/kotlin_modules.rs`
- `crates/sprefa-extract/src/lang/kotlin.rs`
- `crates/sprefa-extract/src/lang/ts_receivers.rs`
- `crates/sprefa-extract/src/lang/ts.rs`
- fixture files the tests above read

## The twelve rows

Weak tests:

1. `tests/130` trait_bound cannot tell `Proj::run` from `Widget::run`. Assert the
   callee SPAN, not the name.
2. `tests/134` three shadow tests share one drop assert that is not keyed by
   caller. Key each assert to its own caller.
3. `tests/134` `use.ts` `crossFileGeneric` has no test. Add one.
4. `tests/131` `dupName` passes even without the module leg. Make it fail when
   the module leg is removed.
5. `tests/131` the `Gadget.kt` comment describes something else. Fix the comment
   to match what the fixture does.

Duplication:

6. `tests/golden_parity.rs` origin-join block is pasted three times, near lines
   1029, 1374 and 1707. Factor it into one helper. Do this row FIRST: two other
   issues are waiting on it.
7. `src/lang/kotlin_modules.rs` `names_by_package` and `package_scope` duplicate
   `declaring_file` at `:192`. Collapse.
8. `src/lang/ts_receivers.rs` keeps a `locals` stack that parallels `scope`. Drop
   the parallel stack and seed `Inferred` for unannotated params instead.
9. `src/lang/kotlin.rs` `module_target` carries `Option` plumbing that no caller
   needs. Remove it.
10. `src/lang/ts.rs` `load_type_params` clears where it should stack. Make it
    stack.

Rows 11 and 12 are whatever remains in the issue body after the ten above. Read
`issues/flash-review-cleanups/item.md` and confirm the count.

Every row is either fixed, or closed with a one-line reason in your report. A row
you skip without a reason is not done.

## Validation, exact command

```
cd ~/projects/hafley-rs/crates/sprefa-extract && cargo test --features cli --no-fail-fast
```

Two `golden_parity` oracle-prefix failures are PRE-EXISTING. Any other failure is
yours. Report both counts.

For rows 1 through 5, prove the test got stronger: break the code the test covers,
show the test now fails, restore the code, show it passes. Paste both outputs.

## Style laws, inline

- Match the surrounding file's style. Do not restructure a file you are editing
  one line of.
- No one-line functions wrapping an array method. Call the array method.
- No `private`. No getter/setter indirection.
- Snapshot assertions use inline snapshots, never a bare "is defined" check.
- Do not denormalize setup shared across many tests into a helper unless the
  issue row asks for it. Row 6 is the one that asks.
- Field names keep one spelling across Rust, the tsp schema, sqlite and CLI
  output. No camelCase twin, no serde rename for casing.
- No em dashes in code comments or commit messages.
- Commit subject: lowercase, names the crate, e.g.
  `sprefa-extract: the trait_bound test asserts a callee span`.

## Reporting contract

One block per row:
- row number, the file and line you changed
- fixed or closed, and if closed, the one-line reason
- for rows 1 to 5, the break/restore proof

Then the final `cargo test` counts.

Stop and report if a row needs a file this brief does not own.
