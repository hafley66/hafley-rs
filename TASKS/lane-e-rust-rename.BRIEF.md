# Lane E: rust rename seats, four gaps closed or stopped

Issue: `issues/lane-e-rust-rename/item.md` (`issuectl show lane-e-rust-rename`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. 16 GB machine.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha.
- Read, in order: `crates/sprefa-extract/AGENTS.md`, `src/lang/rust_rename.rs` whole (1157 lines), `src/lang/rust_receivers.rs:1-40` and `:176-200` and `:532-600`, `src/lang/rust_rehome.rs:268-290` and `:452-470` (the `#[path]` reader), `tests/5_rename_rust.rs` whole, `tests/4_rename_ts.rs:590-730` (the `--verify-scip` test shape), `src/1_rename_verify.rs` whole.
- Build: `cargo build --features cli --bin extract`; binary at `$CARGO_TARGET_DIR/debug/extract`. `rust-analyzer` must be on PATH (`rustup component add rust-analyzer`); `ScipRust` at `src/scip.rs:86` builds the index.
- Scope: rust only.

## Files you own

- `crates/sprefa-extract/src/lang/rust_rename.rs`
- `crates/sprefa-extract/tests/5_rename_rust.rs` (new cases appended)
- `crates/sprefa-extract/tests/fixtures/rust_rename/{path,field,variant,serde,fnuse}/**` (new)

Not yours: `src/types.rs` (`RenameStop`, `SymbolRef`, `SymbolSeat`, `RefRole`, the `Rename` trait at `types.rs:2962-2985`), `src/rename_cx.rs`, `src/1_rename_verify.rs`, `src/lang/rust_receivers.rs` (read it, call `impl_facts`, change nothing), `src/lang/rust_rehome.rs` (read `path_attr` at `:452`, change nothing), `src/lang/rust.rs`.

## The four gaps

Each row is a place where `rust_rename.rs` today either drops a seat it could prove or renames one it cannot.

| gap | today | receipt |
| --- | --- | --- |
| E.1 `#[path]` | `module_path` maps a file to a `ModuleId` by layout alone; a `#[path = "x.rs"] mod m;` file is placed at the wrong module, so `nameable` misses it and its seats are dropped silently | `rust_rename.rs:1035-1059`; `rust_rehome.rs:452-470` already reads the literal |
| E.2 field and variant | `item_ident` declares struct/enum/fn/const/static/trait/type idents; a struct field `pub old: u32` and an enum variant `Old` are never `Decl`s, so `--old size` on `struct Helper { size: u32 }` is `NotFound`; `x.size` is not scanned at all (`visit_expr_method_call` only records `.old(`) | `rust_rename.rs:956-969`, `:932-939`, `:557-558` `methods` |
| E.3 serde spellings | `text_spellings` is the trait default (`types.rs:2982`) returning nothing; `#[serde(rename = "old")]`, `#[serde(alias = "old")]`, and a `"old"` string literal in the corpus never reach the `--text-refs` report | `types.rs:2980-2984`; `ts_rename.rs:116` also returns nothing; `rust_rehome.rs:22` doc names the move-side law |
| E.4 fn-body `use` | `visit_item_use` records the leaf with `chain` only; a `use crate::a::old;` inside a fn body binds the name for that block only, but `harvest` treats it as module-scope (`ours`/`shadowed` at `:386-403`), so a same-named module-scope path outside the block is misjudged | `rust_rename.rs:800-881`, `:381-417`; `Decl.block` at `:585-587` already models the block-scoped shape for items |

## The rule

A seat is emitted when the scope plane or the receiver plane proves the spelling names the anchor. A spelling that only a compiler could bind is a `RenameStop::Dynamic` seat with a `form` naming why, listed before anything is staged. Never a silent skip, never a name-match guess.

### E.1 `#[path]` placement

In `Corpus::open` (`:186`), before `module_path`, build a `BTreeMap<String /*rel of the file the attr names*/, ModuleId>` from every `mod x;` decl carrying `#[path = ".."]` across the corpus (`syn::ItemMod` with `content == None`, attr read by the same rule as `rust_rehome.rs:452`: the literal is relative to the declaring file's directory, or to `<dir>/<mod-name>/` when the declaring file is a non-root non-`mod.rs` file). A file named by such an attr gets `ModuleId = (root of declaring file, declaring chain + [x])`. A file named by two attrs is a stop: `RenameStop::Dynamic` with one seat per attr literal span, form `"path attr twice"`. Files not named fall through to `module_path` unchanged.

### E.2 field and variant seats

Declarations: extend the scan so a `syn::Field` named ident inside `ItemStruct`/`ItemUnion`/a struct-shaped `Variant`, and a `syn::Variant` ident, `declare` with a new `Decl.kind: DeclKind { Item, Method, Field { owner: String }, Variant { owner: String } }` (replace the `method: bool` at `:584` with this enum; `method` reads become `matches!(kind, DeclKind::Method)`).

Seats for a `Field { owner }` anchor:
- `x.old` field access: `syn::ExprField` with `Member::Named(ident)`. Type `x` through the receiver plane's rules (`rust_receivers.rs:9-13`: param annotation, `let x: T`, `self`, a struct field's declared type, one hop through a same-file fn's declared return). `impl_facts` (`:40`) gives the corpus impl table; the local-binding walk you write in `rust_rename.rs` mirrors `rust_receivers.rs:532-600` `tables` + `ReceiverWalk` without touching that file. `Named(owner)` -> seat, role Read (Write when the `ExprField` is the LHS of `ExprAssign`/`ExprAssignOp`). `Named(T != owner)` -> not a seat. `Unknown` -> stop seat, form `"untyped field"`.
- struct literal `Owner { old: v }` and shorthand `Owner { old }` (`syn::ExprStruct` whose path's last segment is `owner` and resolves to the anchor module through `resolve`): seat on the field ident; shorthand becomes `new: old` so the local keeps its name, form `"shorthand"` in the plan row's receipt (`Respell.receipt`).
- struct pattern `Owner { old, .. }` / `Owner { old: p }` (`syn::PatStruct`): same rule, same shorthand law.
- `..` functional update and tuple structs: no seats (a tuple field is an index).
- macro bodies: an `OpaqueToken` with `member == true` (`:575`, `.old` inside a macro) is a stop seat, form `"macro body"`, the same law `:488-498` applies to methods today.

Seats for a `Variant { owner }` anchor: every path whose segments end `[.., owner, old]` after `resolve` reaches the anchor module (`visit_path` at `:883` already records the prefix; match `prefix.last() == owner`). A bare `Old` reached through `use Owner::*` or `use Owner::Old` is a path seat under the existing `use` rules with `chain` extended by `owner`. No receiver typing is needed.

`select_by_at` and the `(None, at_root, decls)` arm at `:52-70`: a field or variant decl is never `at_root` (it is nested in its item), so `--old size` with no `--at` and one field `size` in the anchor picks it through the `(None, [], [one])` arm; two -> `Ambiguous`.

### E.3 serde and string spellings

Implement `text_spellings` on `RustSource` (`rust_rename.rs`, next to `respell_symbol`): return `(rel, spelling)` rows for every string literal in the corpus whose bytes equal `old` when the literal sits in `#[serde(rename = "old")]`, `#[serde(alias = "old")]`, `#[serde(rename_all = ..)]`-derived spelling (skip: report only the literal forms), `#[doc(alias = "old")]`, and any other string literal equal to `old` (the `rustc` rule: a string is never a symbol). Read the attr tokens off `visit_attribute` (`:949`); read bare literals off `syn::visit::visit_lit_str`. Rows are reported by `--text-refs`, never rewritten (the trait doc at `types.rs:2980`).

### E.4 fn-body `use`

`UseLeaf` gains `block: Option<Span>` = `self.blocks.last()` at record time (`:841`, `:855`, `:872`). In `harvest`, a leaf with `block == Some(b)`:
- `Name`/`Alias` reaching the anchor: its seat is emitted as today, and paths inside `b` with a bare or matching chain are `ours` only inside `b` (add the leaf's block to a `local_ours: Vec<Span>` checked beside `ours.contains` at `:435` and `:503`).
- `Name`/`SelfName`/`Shadow` NOT reaching the anchor: `shadowed` only inside `b` (add to `shadow_blocks` at `:357`, which `inside_shadow_block` already consults).
- `Glob` inside a block: today's glob law (`:439-447`) scoped to `b`.
A module-scope path outside `b` is judged by module-scope facts alone.

## Verification, in order

1. Fixtures under `tests/fixtures/rust_rename/<case>/before/` with a hand-written `after/`, judged byte-exact through `diff_rq` like `local/`. Each is a cargo crate (`Cargo.toml` + `src/`), the shape of `local/before/`.
   - `path/`: `src/lib.rs` has `#[path = "elsewhere/impl.rs"] mod util;` and `use crate::util::Helper;`; `src/elsewhere/impl.rs` declares `pub struct Helper`. `--anchor src/elsewhere/impl.rs --old Helper --new Tool` renames the decl and the `use` and every `Helper` path in `lib.rs`. A second file `src/other.rs` with its own `struct Helper` stays.
   - `field/`: `src/util.rs` `pub struct Helper { pub size: u32 }` with `impl Helper { pub fn grow(&mut self) { self.size += 1 } }`; `src/lib.rs` has `fn width(h: &Helper) -> u32 { h.size }`, `let h = Helper { size: 4 }; h.size`, `let Helper { size, .. } = h;`, `fn other(o: &Other) -> u32 { o.size }` with `pub struct Other { pub size: u32 }`. The untyped case is `let v = make(); v.size` where `make` is declared in ANOTHER file (outside the one-hop same-file rule).
     - Case A: `--old size --new width` anchored at `src/util.rs` with `--at` on the field: renames the decl, `self.size`, `h.size` in `width`, the literal key, the pattern key (`size` -> `width: size`), leaves `Other.size` and `o.size`.
     - Case B: with the `make()` file present -> `RenameStop::Dynamic`, one seat, form `untyped field`, dry-run count 0. Keep case B as its own fixture dir `field_stop/`.
   - `variant/`: `pub enum Kind { Old, New }`, seats `Kind::Old`, `use Kind::Old; Old`, a match arm `Kind::Old =>`, and `mod other { pub enum Kind { Old } }` untouched.
   - `serde/`: `#[derive(Serialize)] struct S { #[serde(rename = "size")] len: u32 }` beside a `struct Helper { size: u32 }`; `--old size` renames the field seats and `--text-refs` reports `src/util.rs` `"size"` once; the `after/` tree keeps the literal.
   - `fnuse/`: `src/lib.rs` `fn a() { use crate::util::Helper; Helper::new() }` and, at module scope, `struct Helper;` with `fn b() -> Helper { Helper }`. `--anchor src/util.rs --old Helper --new Tool`: the `use` and the path in `a` rename; `b`'s two spellings stay.
2. `cargo check` on every committed `after/` crate, the way `renamed_fixture_crate_passes_cargo_check` (`5_rename_rust.rs:188`) does; one test per fixture, or extend that test to loop over the five.
3. `--verify-scip` on `field/` case A and `variant/`: mirror `scip_verify_agrees_on_the_exports_fixture` (`4_rename_ts.rs:630`) with `ScipRust.build(root)`; assert `scip-verify disagreements=0`. rust-analyzer binds fields and variants, so it is the oracle for E.2. Measure the index time and `#[ignore]` only if over 10 s, stating the number in REPORT.md.
4. `cargo test --features cli --test 5_rename_rust --test 3_move_rust --test 71_rust_paths --test golden_parity`.
5. Full gate.

## Commits

Subjects exactly:

- `feat(extract): rust rename places #[path] modules and scopes fn-body use`
- `feat(extract): rust rename field and variant seats through the receiver plane`
- `feat(extract): rust rename reports serde and string spellings as text refs`
- `test(extract): rust rename fixtures for path, field, variant, serde, fn-body use`

Trailer on each: `Refs-Issue: @lane-e-rust-rename`. Do not push.

## Deliverable

`REPORT.md` at the worktree root. Overwrite the stale one there. Tables only. Sections:

1. `## Commits` : sha, subject, files.
2. `## Seats` : per fixture case: seat span, form, role, renamed or stopped, why.
3. `## Verify` : cargo check output per `after/` crate, scip-verify rows, index seconds.
4. `## Tests changed` : test, old expectation, new expectation, why.
5. `## Gate` : last 3 lines.
6. `## Blocked` : empty, or the exact error and the diff you wanted.

## Laws

- Stop and write `## Blocked` when a command fails in a way this brief did not anticipate.
- No em dashes. No praise. Facts and receipts.
- Rust comments: at most 2 consecutive comment lines; state only constraints the code cannot show.
- Tests are integration tests through the real `extract` binary on fixture crates, judged against hand-written `after/` trees and `cargo check`. No mocks, no fakes. Expected values are hand-derived, never copied from the binary's output.
- A seat the plane cannot type is a stop. Never rename by name match.
- `rust_receivers.rs` is read-only for you: copy its rules, do not import its private types.
