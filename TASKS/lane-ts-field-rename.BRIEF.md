# Lane TF: ts field rename, property seats typed by the receiver plane

Issue: `issues/ts-field-rename/item.md` (`issuectl show ts-field-rename`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. Never run the full gate in parallel with a build. 16 GB machine.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha.
- Read, in order: `crates/sprefa-extract/AGENTS.md`, `src/lang/ts_rename.rs` whole (521 lines), `src/lang/ts_receivers.rs:1-120` and `:400-570` (the walk), `tests/4_rename_ts.rs` whole, `plans/reviews/2026-09-18-lane-d-untyped-REPORT.md` section "ts import exemption".
- Build: `cargo build --features cli --bin extract`; binary at `$CARGO_TARGET_DIR/debug/extract`. `tsc` must be on PATH (the existing `tsc_is_clean_on_the_committed_tree` test uses it).
- Scope: ts only.

## Files you own

- `crates/sprefa-extract/src/lang/ts_rename.rs`
- `crates/sprefa-extract/src/lang/ts_receivers.rs` (additive: member-expression rows; do not change call-site row semantics, lane D depends on them)
- `crates/sprefa-extract/tests/4_rename_ts.rs` (new cases appended)
- `crates/sprefa-extract/tests/fixtures/ts_rename/fields/**` (new)

Not yours: `src/types.rs` (`RenameStop`, `SymbolRef`, `SymbolSeat`, `RefRole` at `types.rs:2860-2925` are what you emit), `src/rename_cx.rs`, `src/bin/extract.rs`, `ts.rs`.

## The defect

`ts_rename.rs:31-100` `symbol_refs` renames a scope-plane binding: `oxc_semantic` symbols (variables, functions, classes, imports). A class field, interface property, or object key is not a binding, so `extract rename --anchor a.ts --old count --new total` on `class C { count = 0 }` returns `NotFound`; and any `x.count` in the anchor is a `DynamicSeat` (`ts_rename.rs:409-457`) that stops every run touching that name.

## The rule

A rename whose `--at` lands inside a property declaration is a FIELD rename. Its seats are every spelling of `old` that the receiver plane proves belongs to the owner type, across the anchor and every file that imports the owner. A member spelling of `old` on a receiver the plane cannot type is a stop (`RenameStop::Dynamic`, listing every such seat with `form` = `"untyped member"`), never a silent skip and never a guess.

### TF.1 the anchor: what `--at` names

Walk the anchor with `oxc_ast_visit::Visit`; the declaration under `request.at` is one of:
- `PropertyDefinition` in a `Class` (`count = 0`, `private count: number`, `static count`)
- `MethodDefinition` in a `Class` (a method is a field for this lane; `get`/`set` accessor pairs are ONE field: both seats)
- `TSPropertySignature` / `TSMethodSignature` in a `TSInterfaceDeclaration` or a `TSTypeLiteral` that a `TSTypeAliasDeclaration` names
- `ObjectProperty` key in an `ObjectExpression` bound by `const x = { ... }` (owner = the binding `x`, a value owner)
- a constructor parameter property `constructor(public count: number)`

Owner = the class / interface / type-alias / binding name. `--at` absent and `old` matching exactly one property declaration in the anchor -> that one; more -> `Ambiguous` listing them (existing `ambiguous()` helper). A field rename never touches a same-named scope binding (`const count = 1` elsewhere in the file stays).

### TF.2 receiver rows for member expressions (`ts_receivers.rs`)

Today `visit_call_expression` (`ts_receivers.rs:~520-580`) records `(start, end, TypeBinding)` only for call sites. Add a second row set `member_rows: Vec<(u32, u32, TypeBinding)>` keyed on the PROPERTY span of every `StaticMemberExpression` (`recv.old`), the same `receiver_of` logic, plus `this` -> `Decl(<enclosing class>)` (already at `:190-194`). Also record `ComputedMemberExpression` with a string literal key (`recv["old"]`) the same way. Expose `member_receiver(blob, span) -> Option<&TypeBinding>` beside `facts_of`. Call-site rows stay byte-identical.

### TF.3 seats

For the owner `O` declared in the anchor:
- in the anchor: every member row on `old` whose binding is `Decl(O)`, or `Decl(S)` where `S` `extends`/`implements` `O` in the corpus (one hop; `ts.rs` type-edge candidates carry `extends`/`implements`, or scan the anchor's classes), or `Field(O, f)` resolving to a property typed `O`; every `this.old` inside `O`'s body; destructuring `const { old } = expr` where `expr`'s binding is `Decl(O)` (seat = the pattern key; a renamed shorthand `{ old }` becomes `{ new: old }` so the local keeps its name, form `"destructure"`); object literals passed where `O` is the declared parameter type or the declared variable type (`const o: O = { old: 1 }`, seat = the key).
- in importers: files that import `O` from the anchor (`ts_rehome.rs:33` `import_refs` gives the importer set, as `importer_refs` at `ts_rename.rs:206` uses it): the same rules over each importer with `O` bound through its import name or alias.
- type positions: `O["old"]` indexed access, `keyof O` narrowing literal `"old"`, `Pick<O, "old">`: seat = the string literal.
- `RefRole`: Write for assignment targets and the declaration, Read otherwise (`role_of` at `ts_rename.rs:467`).

Stops: any member row on `old` (anchor or importer) whose binding is `Inferred`, `Ambiguous`, `Shadowed`, or `Decl(T)` for a `T` the corpus does not declare -> collect into `RenameStop::Dynamic` with form `"untyped member"`; return the stop before staging anything. A member row on `old` with `Decl(T)` for a corpus type `T` unrelated to `O` is NOT a seat and NOT a stop (a different field with the same name).

### TF.4 `respell_symbol` and `text_spellings`

`respell_symbol` (`ts_rename.rs:102`) already replaces `old` at each seat span. `text_spellings` (`:116`) returns nothing today; keep that.

## Verification, in order

1. Fixtures `tests/fixtures/ts_rename/fields/before/` and a hand-written `after/`, judged byte-exact like `local/`:
   - `owner.ts`: `export class Counter { count = 0; bump() { this.count += 1; return this.count } }` ; `export interface Shape { count: number }` ; `export class Sub extends Counter { peek() { return this.count } }` ; `const count = 9; export function unrelated() { return count }` ; `export class Other { count = "x" }` ; `function other(o: Other) { return o.count }`.
   - `use.ts`: `import { Counter, Shape } from "./owner.js"; export function read(c: Counter) { return c.count } ; export function shape(s: Shape) { return s.count } ; export function pick(c: Counter) { const { count } = c; return count } ; export function lit(): Shape { return { count: 1 } } ; type K = Counter["count"];`
   - Case A: `--anchor owner.ts --old count --new total --at <offset of Counter's count>`: renames `Counter.count` decl, both `this.count` in `bump`, `this.count` in `Sub.peek`, `c.count` in `read`, `{ count }` -> `{ total: count }` in `pick`, `Counter["count"]`; leaves `Shape.count`, `s.count`, `{ count: 1 }` in `lit` (Shape), `const count`, `Other.count`, `o.count` untouched. Assert the byte-exact `after/` tree.
   - Case B: `stops/`: `export function loose(x) { return x.count }` in an importer -> `RenameStop::Dynamic` with one `untyped member` seat, nothing written (dry-run count 0).
   - Case C: `--at` on `Shape.count` renames `Shape.count`, `s.count`, `{ count: 1 }` in `lit`, and nothing of `Counter`.
   - Case D: no `--at`, `old = count` in `owner.ts` -> `Ambiguous` listing the three declarations (Counter, Shape, Other) and the const.
2. `tsc --noEmit` clean on every committed `after/` tree (extend `tsc_is_clean_on_the_committed_tree`).
3. `scip_verify` on case A: extend `scip_verify_agrees_on_the_exports_fixture`'s pattern to `fields/`; the index must agree on every seat (scip-typescript binds properties, so this is the oracle for this lane).
4. `cargo test --features cli --test 4_rename_ts --test 40_ts_resolve --test 136_untyped_receiver_ts --test 134_ts_binding_legs --test golden_parity`.
5. Full gate.

## Commits

Subjects exactly:

- `feat(extract): ts receiver plane records member-expression receivers`
- `feat(extract): ts field rename over receiver-typed property seats`
- `test(extract): ts field rename fixtures, stops, tsc and scip verify`

Trailer on each: `Refs-Issue: @ts-field-rename`. Do not push.

## Deliverable

`REPORT.md` at the worktree root. Tables only. Sections:

1. `## Commits` : sha, subject, files.
2. `## Seats` : per fixture case: seat span, form, role, renamed or stopped, why.
3. `## Verify` : tsc output, scip_verify output.
4. `## Tests changed` : test, old expectation, new expectation, why.
5. `## Gate` : last 3 lines.
6. `## Blocked` : empty, or the exact error and the diff you wanted.

## Laws

- Stop and write `## Blocked` when a command fails in a way this brief did not anticipate.
- No em dashes. No praise. Facts and receipts.
- Rust comments: at most 2 consecutive comment lines; state only constraints the code cannot show.
- Tests are integration tests through the real `extract` binary on fixture files, judged against hand-written `after/` trees. No mocks, no fakes. Expected values are hand-derived, never copied from the binary's output.
- A seat the plane cannot type is a stop. Never rename by name match.
