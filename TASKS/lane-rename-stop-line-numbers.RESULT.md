# lane-rename-stop-line-numbers RESULT

`RenameStop::Dynamic` seats print `file:line` and a route seat names the file it
reaches. Issue criteria 1 and 2 closed; 3 through 6 untouched (`needs-chris`).

## The diagnostic, before and after

On the crate's own tree (`rename src/types.rs#ExtractOutput RyiOutput`), before:

```
src/bin/extract.rs byte 1905: path attr twice reaches the symbol at runtime
tests/0_sqlite.rs byte 56: path attr twice reaches the symbol at runtime
```

After, same tree, same two seats:

```
src/bin/extract.rs:40: path attr twice reaches src/bin/extract/0_sqlite.rs at runtime
tests/0_sqlite.rs:4: path attr twice reaches src/bin/extract/0_sqlite.rs at runtime
```

Bare seats (no route) render the same `file:line` form with the symbol named:

```
src/app.ts:3: computed member reaches the symbol at runtime
```

Measured on the lane's two-route fixture, verbatim pairs: before
`src/bin/extract.rs byte 9: path attr twice reaches the symbol at runtime` /
`tests/probe.rs byte 59: path attr twice reaches the symbol at runtime`; after
`src/bin/extract.rs:1: path attr twice reaches src/bin/home.rs at runtime` /
`tests/probe.rs:3: path attr twice reaches src/bin/home.rs at runtime`.

## Tests

`crates/sprefa-extract/tests/38_rename_stop_lines.rs`, five cases, all passing:

1. `the_stop_prints_file_and_line_not_a_byte_offset`
2. `an_attr_on_the_first_line_prints_line_one`
3. `the_stop_names_the_file_the_route_reaches`
4. `other_arms_keep_the_bare_file_and_line_form`
5. `the_two_route_stop_still_exits_six`

Fail-first held: against the byte-offset binary the line cases failed with
`byte 9`/`byte 59`/`byte 49` in the receipts (commit 819f89a9).

## Construction sites threaded

`SymbolSeat` gained `line` (one-based, resolved at construction against the
file's line table) and `reaches` (the reached file, empty when the seat is not
about a route). `grep -rn "SymbolSeat {" src/` shows 11 hits: the definition
plus exactly these 10 sites, each threaded:

- `src/types.rs:2906` struct definition; `:2957-2971` the `Dynamic` Display arm
- `src/lang/rust_rename.rs:47` the `path_stops` remap (`line`/`reaches` copied)
- `src/lang/rust_rename.rs:566` glob import, paths loop
- `src/lang/rust_rename.rs:588` untyped field access
- `src/lang/rust_rename.rs:644` glob import, opaque loop
- `src/lang/rust_rename.rs:664` macro body
- `src/lang/rust_rename.rs:697` glob import, bare-token loop
- `src/lang/rust_rename.rs:2002` path attr twice, `reaches: file.clone()`; the
  line resolves in `path_decls:2046` where `line_starts` is live and rides the
  `(String, Span, u32, ModuleId)` tuple
- `src/lang/ts_rename.rs:433` computed member / member access
- `src/lang/kotlin_rename.rs:229` wildcard import
- `src/lang/prolog/_2_rename.rs:335` runtime seats

Each arm resolves the line with
`line_starts.partition_point(|start| *start <= span.start) as u32` against a
`build_line_starts` table, the same helper the rust arm already used; no new
scanner, no new dependency.

## Pins updated in place

The four live assertions pinning `{file} byte {offset}` became `{file}:{line}: `,
one assertion each, none deleted, none duplicated:

- `tests/4_rename_ts.rs:370` (`dynamic_stop_lists_every_seat`)
- `tests/5_rename_rust.rs:156` (`glob_importer_is_a_dynamic_stop`)
- `tests/7_rename_kotlin.rs:157` (`wildcard_importer_is_a_dynamic_stop`)
- `tests/8_rename_prolog.rs:154` (the `=..` seat)

## Gate

```
cargo test --features cli 2>&1 | grep -E "^test result:" | summed
187 binaries, 1012 passed, 0 failed
```

Baseline before the lane: 1007 passed, 0 failed across 186 binaries; the delta
is exactly the five new tests in the new binary. Every per-binary line summed;
no line reports a failure.

```
cargo metadata --locked --format-version 1 >/dev/null && echo LOCK_OK
LOCK_OK
```

## Out of scope, held

Exit code 6 (`src/0_rename.rs:49`), the refuse-vs-plan branch
(`src/lang/rust_rename.rs:1995-2012`, seats only), the union-vs-refuse decision
(issue criteria 3-4, `needs-chris`), and the concurrent lane's
`src/lang/2_source_query.rs` / `tests/37_*`, never opened.
