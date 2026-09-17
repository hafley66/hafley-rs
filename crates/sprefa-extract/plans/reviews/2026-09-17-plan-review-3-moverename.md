# Adversarial review 3 of 3: move/rename at 100 without a compiler

Inputs: `plans/2026-09-17-fast-slow-parity-and-move-rename.md` lane 4 and section 7;
`docs/0_architecture-matrix-20260917.md` sections 3, 5a, 7. Receipts are `file:line` inside `crates/sprefa-extract/` in this worktree.

## Move: what the module plane cannot see

| grade | meaning |
|---|---|
| deterministic | the module plane, a manifest, or a literal path at the site decides it; no checker needed |
| spelled-receiver-only | correct only because the path or specifier is written literally at that site |
| needs-types | needs a checker, a macro expander, or a language front end this crate does not have |
| needs-runtime | only the running program knows the spelling |

Column 4 (`can phase 1 detect it`) means: can a per-file parse see the construct at all. Column 5 means: does any arm rewrite it today, or does the plan propose to.

| language | construct | example | can phase 1 detect it | can it be rewritten | grade | receipt |
|---|---|---|---|---|---|---|
| rust | `mod name;` decl whose file leaves its dir | `mod b;` in `src/lib.rs`, `src/b.rs` -> `src/deep/b.rs` | yes | yes: `#[path]` grown | deterministic | rust_rehome.rs mod_respell; 3_move_rust.rs:145 |
| rust | existing `#[path]` literal | `#[path = "b.rs"] mod b;` | yes | yes: respelled | deterministic | 3_move_rust.rs:171 |
| rust | `mod.rs` directory-standing file | `src/a/mod.rs` -> `src/a.rs` | yes | yes | deterministic | rust_rehome.rs:82 directory_stem |
| rust | `include!`/`include_str!`/`include_bytes!` literal arg | `include_str!("./tpl.txt")` | yes | yes: dir-relative | spelled-receiver-only | rust_rehome.rs:38; 3_move_rust.rs:187 |
| rust | non-literal include arg | `include!(concat!(env!("CARGO_MANIFEST_DIR"), "/x.rs"))` | yes: visibly not a literal | no | needs-types | src/bin/extract/0_sqlite.rs:12-20 is this crate's own spelling |
| rust | Cargo.toml target `path` + `[package] build` | `[[bin]] path = "src/bin/x.rs"` | yes | yes | deterministic | manifest_leaves; 3_move_rust.rs:215 |
| rust | `[workspace] members`, `[dependencies] path`, `[patch]` | `members = ["crates/a"]` | yes | no: TARGET_TABLES excludes them | deterministic | rust_rehome.rs:44-47 comment |
| rust | crate-target identity by path shape | `src/tool.rs` -> `src/bin/tool.rs` mints a new bin target | yes | no: RehomePlanCheck returns only relocate errors | deterministic | auto_crate_root :1563; RehomePlanCheck :211 |
| rust | `use crate::m::F` when the module name survives | `use crate::b::f;` | yes | yes by holding the tree still (USE_PATH respell None :199) | deterministic | rust_rehome.rs:200; 3_move_rust.rs:201 |
| rust | `--relocate-mod` tree move + `pub(crate)` widening | `src/a.rs` -> `src/util/a.rs` | yes | yes | deterministic | 3_move_rust.rs:266,289,373 |
| rust | proc-macro generated paths | `quote!{ use crate::b::F; }` in a derive | partial: macro token streams | no | needs-types | IncludeScan visits literal macro args only |
| rust | macro-name file tables | `sqlx::query_file!("sql/q.sql")`, `include_dir!("tpl")` | partial: literal arg visible, macro unknown | no | spelled-receiver-only | INCLUDE_MACROS lists 3 macros |
| ts | static ESM/CJS specifier | `import {A} from '../a'` | yes | yes | deterministic | ts_rehome.rs:141; 41_move_ts.rs:128 |
| ts | tsconfig `paths` alias covering the destination | `import {A} from '@app/a'` | yes | yes: alias kept when the prefix still covers, else relative | deterministic | alias_respell ts_rehome.rs:225 |
| ts | package.json `exports`/`main`/`bin` subpaths | `"exports": {"./a": "./src/a.ts"}` | yes | yes | deterministic | CANDIDATE_FIELDS ts_rehome.rs:24 |
| ts | relative path constant in a file that STAYS while the target moves | `new URL('../a.css', import.meta.url)` | yes | no: ts_path_literals runs only for a moving file | spelled-receiver-only | ts_rehome.rs:158 `if is_moved` |
| ts | `require.context('./dir')` in a moving file | `require.context('./views', true, /\.ts$/)` | yes | yes: literal respell | spelled-receiver-only | literal_respell ts_rehome.rs:190 |
| ts | dynamic import with a computed/template arg | import(`./${x}`) | no: not a static specifier | no | needs-runtime | TsSpecifier requires a static module string |
| ts | tsconfig.json `references[].path` / `paths` / `rootDir` / `include` | `{"path": "../lib"}` | yes | no: only package.json is a manifest | deterministic | ts_rehome.rs:89 |
| ts | jest `moduleNameMapper` / jest.config paths | `"^@app/(.*)$": "<rootDir>/src/$1"` | yes | no | deterministic | not in CANDIDATE_FIELDS |
| ts | `.d.ts` twin of a moved `.ts` | `src/a.ts` -> `src/lib/a.ts` leaves `src/a.d.ts` | yes: sibling identity | no | deterministic | EXTENSIONS ts_resolve.rs:31 |
| ts | `dist/` emitted twin of a moved specifier | `dist/a.js` | yes | no: EMITTED_DIR excluded; --text-refs report only | deterministic | ts_rehome.rs:31,125 |
| go | import path in another package | `import "example.com/m/a"` | yes: go_module_specifiers | no arm | deterministic | go.rs:754; proposal 4.1 |
| go | `go.mod` module/require/replace lines | `module example.com/m` | yes | no arm | deterministic | proposal 4.1 RehomeManifests |
| go | `internal/` visibility rule | `example.com/m/internal/x` importable only under `m/` | yes | no check | deterministic | no arm |
| go | test package in the same dir | `a/foo.go` -> `b/foo.go` leaves `a/foo_test.go` | yes | no | deterministic | no arm |
| go | `//go:embed` pattern | `//go:embed assets/*` | no: comment, no row | no | spelled-receiver-only | no arm |
| go | `go.work` use directives | `use ./a` | yes | no | deterministic | no arm |
| go | vendor/ tree mirroring the module path | `vendor/example.com/m/a/x.go` | yes | no | deterministic | no arm |
| go | package clause vs destination directory name | moving `a/x.go` (package a) into a dir holding package b | yes | no: no package-decl respell planned | deterministic | proposal 4.1 respells import paths only |
| go | string paths read at runtime | `os.ReadFile("tmpl/x.tmpl")`, `template.ParseFiles` | partial: string literal visible | no | needs-runtime | no arm |
| go | build tags, cgo, generated names | `//go:build linux` | yes | no | deterministic | no arm |
| kotlin | explicit import | `import a.b.C` | yes: kt_walk_import_headers | yes | deterministic | 4_move_kotlin.rs:137 |
| kotlin | moved file's own `package a.b` line | `package a.b` | yes | yes | deterministic | 4_move_kotlin.rs:155 |
| kotlin | wildcard import | `import a.b.*` | yes | no: counted and warned | needs-types | kotlin_rehome.rs:100 |
| kotlin | same-package bare use across files | two files in `a` where one leaves | yes | no: counted only | deterministic | kotlin_rehome.rs:88 |
| kotlin | Gradle source-set roots | `sourceSets.main.kotlin.srcDirs` | yes | no | deterministic | manifests leg is None (header) |
| kotlin | string FQ name in an annotation arg or reflection | `Class.forName("a.b.C")` | yes: reported seat | no: reported never rewritten | needs-runtime | kotlin_rename.rs header, text_spellings |
| kotlin | `@file:JvmName` / `@JvmName` | `@file:JvmName("Util")` | yes | no | needs-runtime | annotation args are seats, not targets |
| kotlin | `typealias` forwarding a moved decl | `typealias C = a.b.C` | yes | no | deterministic | no shim by decision (header) |
| kotlin | layout disagrees with the declared package | file at `src/x.kt` declaring `package a` | yes | named stop | deterministic | 4_move_kotlin.rs:186 |
| python | `import a.b`, `import a.b as c` | `import pkg.mod as m` | yes: py_module_specifiers | no arm | deterministic | _0_source.rs:1545; proposal 4.3 |
| python | `from a.b import n` | `from pkg.mod import f` | yes | no arm | deterministic | proposal 4.3 |
| python | relative import depth when the file moves deeper | `pkg/a.py` -> `pkg/sub/a.py` with `from . import b` | yes | no: dots are not recomputed | deterministic | _2_modules.rs:9 (dots kept as written) |
| python | `pkg/__init__.py` directory-standing file | `pkg/__init__.py` -> `other/__init__.py` | yes | no: directory_stem defaults to None | deterministic | types.rs:2801 default |
| python | PEP 420 namespace package dir | dir with no `__init__.py` | yes | no | deterministic | _2_modules.rs header |
| python | `pyproject.toml`/`setup.py` entry points and packages list | `[project.scripts] x = "pkg.mod:main"` | yes | no: no manifests leg proposed | deterministic | proposal 4.3 has no RehomeManifests |
| python | `conftest.py` / pytest rootdir relative paths | `--cov=pkg` in `pyproject.toml` | yes | no | deterministic | no arm |
| python | `importlib.import_module("a.b")`, `__import__` | `importlib.import_module("pkg.mod")` | yes: string literal visible | no | spelled-receiver-only | no arm |
| python | `__all__ = ["n"]` re-export strings | `__all__ = ["f"]` | yes | no | spelled-receiver-only | no arm |
| python | Django style dotted strings | `INSTALLED_APPS = ["pkg.apps"]` | yes | no | spelled-receiver-only | no arm |
| python | data files addressed through the package | `importlib.resources.files("pkg")` | yes | no | spelled-receiver-only | no arm |
| python | `.pyi` twin / `py.typed` | `pkg/mod.pyi` beside `pkg/mod.py` | yes: sibling identity | no | deterministic | _0_source.rs matches `.pyi` |

## Rename: what needs types

Column 5 asks whether a `RenameStop` can be reached from facts the crate already has (phase-1 nodes plus the module/type planes), not whether the arm reaches it today.

| language | symbol kind | construct | example | grade | can RenameStop be triggered from phase-1 facts | receipt |
|---|---|---|---|---|---|---|
| rust | top-level | item ident, `use` trailing segment, `ExprPath`/`TypePath` segment | `use crate::b::{Helper};` + `b::Helper` | deterministic | n/a | 5_rename_rust.rs:118,188 |
| rust | top-level | glob `use m::*` importer writes the bare name | `use crate::b::*;` then `Helper` | deterministic | yes: Dynamic seats (module plane could decide it) | 5_rename_rust.rs:139 |
| rust | top-level | macro body bare ident, shadowed or block-local | `macro_rules! m { () => { Helper() } }` | deterministic | no: classified by token context | 5_rename_rust.rs:212,226 |
| rust | top-level | module read by `#[path]` layout | `#[path = "a/deep.rs"] mod a;` | deterministic | no: layout-only law drops seats silently | rust_rename.rs:12-16, module_path |
| rust | top-level | block-written `use` counted against the enclosing MODULE scope | `fn f() { use crate::b::Helper; }` | deterministic | no: stated limit | rust_rename.rs:20-23 |
| rust | member | inherent method, call sites in other files | `x.old()` in `src/c.rs` for `impl A { fn old }` | needs-types | yes in principle: the per-file scan already collects `x.old()` spans, harvest consumes them only for the anchor file | rust_rename.rs:515 with anchored set at :61 |
| rust | member | trait method vs `impl Trait for X` method | `trait T { fn old }` + `impl T for A { fn old }` | needs-types | partial: `method_owner.trait` is filled in the type plane | type plane types.rs:202-550; method_owner is rust-only (docs 5c) |
| rust | member | struct field and enum variant | `struct S { old: u8 }`, `x.old` | needs-types | yes: today NotFound (no field/variant decl scan) | rust_rename.rs has no visit_field/visit_variant |
| rust | member | derive-generated spellings and serde attribute strings | `#[serde(rename = "old")]`, `#[derive(Serialize)]` | needs-types | no: rust arm's text_spellings is the empty default | types.rs:2975 text_spellings default (empty) |
| ts | top-level | exported binding, named-import importers | `export const old` + `import {old}` | deterministic | n/a | 4_rename_ts.rs:389 |
| ts | top-level | aliased import clause | `import {old as local}` | deterministic | n/a | 4_rename_ts.rs:408 |
| ts | top-level | re-export relay | `export {old} from './lib'`, `export *` | deterministic | n/a | importer_seats ts_rename.rs:227 |
| ts | top-level | namespace import member access in an importer | `import * as lib; lib.old` | spelled-receiver-only | no: dynamic_seats scans the anchor file only; scip verify reports the miss | 4_rename_ts.rs:650; ts_rename.rs:415 |
| ts | top-level | default import clause | `import old from './lib'` | spelled-receiver-only | no: importer_seats matches ImportSpecifier only | ts_rename.rs:250 |
| ts | top-level | declaration merging | `interface Foo` + `const Foo` | deterministic | yes: Ambiguous via symbol_redeclarations | ts_rename.rs:78 |
| ts | top-level | computed string key in the anchor file | `obj["old"]` | needs-runtime | yes: Dynamic seat | ts_rename.rs:440 |
| ts | top-level | computed string key in an importer | `lib["old"]` in `src/other.ts` | needs-runtime | no: importers deliberately outside the scan | ts_rename.rs:415 comment |
| ts | top-level | JSX element name | `<old />` | deterministic | no: oxc reports JSX identifiers as references [INFERENCE] | binding_refs ts_rename.rs:155 |
| ts | top-level | declaration twin in `.d.ts` | `src/lib.d.ts` re-declares the export | deterministic | no: no twin law | importer graph resolves the twin as its own module |
| ts | member | class/interface method or property | `obj.old()` where `old` is a method | needs-types | yes: NotFound (class members are not scope bindings) | ts_rename.rs:38-52 bindings only |
| ts | member | `keyof T` / mapped-type / string-literal union members | `type K = keyof T`, `"old"` in a union | needs-types | no | no property-name plane |
| go | top-level | exported package-scope ident reached as `pkg.Name` | `pkg.Old()` in another package | deterministic | n/a (proposed arm 4.2) | proposal 4.2 |
| go | top-level | unexported ident used inside the package dir | `Old()` in a sibling file | deterministic | n/a | proposal 4.2 |
| go | top-level | exportedness change (`Old` -> `old`) across importers | `pkg.Old` -> `pkg.old` | deterministic | no: no check proposed | proposal 4.2 names exported names only |
| go | top-level | reflection and template field names | `reflect.Value.MethodByName("Old")`, `{{.Old}}` | needs-runtime | no: strings are not scanned | proposal 4.2 is tree-sitter scopes plus the package index |
| go | member | method rename with a spelled receiver | `var a A; a.Old()` | spelled-receiver-only | yes: the plan's rule stops without a spelled receiver | proposal 4.2 |
| go | member | interface satisfaction | `type I interface { Old() }` + `func (A) Old()` | needs-types | no: nothing in phase-1 facts relates a struct method to an interface method | go_modules.rs has no method-set rows |
| go | member | embedded struct promoted method | `A{B}; a.Old()` | needs-types | no | no promotion law |
| go | member | method value (no call parens) | `f := a.Old` | needs-types | no: the plan's rule keys on call sites | proposal 4.2 |
| go | member | struct tag strings | `json:"old"`, `yaml:"old"` | needs-runtime | no | no string plane for tags |
| kotlin | top-level | declaration ident, import trailing segment, FQ `a.b.OLD` | `import a.b.Helper` + `a.b.Helper` | deterministic | n/a | 7_rename_kotlin.rs:115 |
| kotlin | top-level | alias clause | `import a.b.Helper as H` | deterministic | n/a | 7_rename_kotlin.rs:115 header |
| kotlin | top-level | wildcard import | `import a.b.*` | deterministic | yes: Dynamic (module plane knows the package) | 7_rename_kotlin.rs:141 |
| kotlin | top-level | `@JvmName` and annotation args | `@JvmName("OLD")` | spelled-receiver-only | no: reported through text_spellings only | kotlin_rename.rs header |
| kotlin | top-level | Java callers of the renamed declaration | `UtilKt.Helper()` from `Main.java` | needs-types | no: no java front end in the roster | sources()/roster has no java |
| kotlin | member | extension function called on a value receiver | `fun String.old()` + `s.old()` | needs-types | no: ident_seat drops a navigation whose receiver is an expression | kotlin_rename.rs:405-410,449 |
| kotlin | member | member function call sites on a variable receiver | `x.old()` in another package | needs-types | no: same drop | kotlin_rename.rs:449 |
| kotlin | member | overloads | `fun old(Int)` + `fun old(String)` | needs-types | yes in principle: two decls give Ambiguous, but after --at the rewrite is by name and hits both | kotlin_rename.rs:56-60 |
| kotlin | member | local or member binding reusing the name | `val old = 1` inside a function body | needs-types | no: stated limit, only top-level decls are read as shadows | kotlin_rename.rs:20-22 |
| python | top-level | module-level def/class reached by `from m import old` | `from pkg.mod import old` | deterministic | n/a (proposed arm 4.3) | proposal 4.3 |
| python | top-level | `import m` then `m.old` | `import pkg.mod as m; m.old()` | spelled-receiver-only | no: attribute access is not an import clause | proposal 4.3 covers clauses only |
| python | top-level | `__init__.py` re-export | `from .mod import old` in `__init__.py` | deterministic | n/a | proposal 4.3 |
| python | top-level | `__all__` string list | `__all__ = ["old"]` | spelled-receiver-only | no: strings are not clauses | proposal 4.3 |
| python | top-level | `getattr`/`importlib` dynamic reach | `getattr(m, "old")` | needs-runtime | no | proposal 4.3 |
| python | top-level | monkeypatch by attribute path | `m.old = fake` in a test | needs-runtime | no | proposal 4.3 |
| python | top-level | `getattr(obj, "old")` on an instance | `getattr(obj, "old")()` | needs-runtime | no | proposal 4.3 |
| python | member | method rename, call sites on non-spelled receivers | `self.old()` / `obj.old()` | needs-types | yes: a stop is implementable (any receiver not written) | no arm exists |
| python | member | `@property` vs same-named instance attribute | `@property def old` + `self.old = 1` | needs-types | no | no arm exists |
| python | member | dataclass field | `@dataclass class C: old: int`, `C(old=1)` | needs-types | no | no arm exists |
| python | member | duck typing (caller has no declared type) | `def use(x): return x.old()` | needs-types | no | no arm exists |
| python | member | pydantic/attrs/`__slots__` field names | `__slots__ = ("old",)` | needs-runtime | no | no arm exists |

## Coverage of existing tests

| language | construct | covered by test (file + fn) or none |
|---|---|---|
| rust | move: `mod` decl gains `#[path]` when its file leaves the dir | 3_move_rust.rs a_mod_decl_gains_a_path_attr_when_its_file_leaves_its_dir |
| rust | move: existing `#[path]` literal respelled | 3_move_rust.rs an_existing_path_attr_is_respelled |
| rust | move: `mod.rs` form is a no-op | 3_move_rust.rs a_move_to_the_mod_rs_form_changes_nothing |
| rust | move: `include_str!` literal respelled | 3_move_rust.rs an_include_str_literal_is_respelled |
| rust | move: `use` path survives while the module name survives | 3_move_rust.rs a_use_path_survives_when_the_mod_name_survives |
| rust | move: Cargo.toml `[[bin]] path` follows | 3_move_rust.rs cargo_toml_bin_path_follows_the_move |
| rust | move: `--relocate-mod` decl move + crate-wide `use` rewrite | 3_move_rust.rs relocate_mod_moves_the_decl_into_the_new_parent, relocate_mod_respells_use_paths_crate_wide, relocate_mod_with_no_parent_module_is_a_named_error, default_strategy_is_unchanged, relocate_mod_leaves_the_fixture_compiling |
| rust | move: `pub(crate)` widening on relocation | 3_move_rust.rs a_private_fn_used_by_a_sibling_after_relocation_becomes_pub_crate, a_private_fn_used_only_inside_its_module_stays_private, an_already_pub_item_is_untouched, without_relocate_mod_nothing_is_widened, relocate_promotion_leaves_the_fixture_compiling |
| rust | move: dry run touches nothing | 3_move_rust.rs dry_run_prints_every_respell_and_touches_nothing |
| rust | move: non-literal include arg (`concat!`+`env!`) | none |
| rust | move: `[workspace] members` / `[dependencies] path` | none |
| rust | move: crate-target identity minted by a path shape | none |
| rust | move: proc-macro `quote!` paths | none |
| rust | rename: item + `use` + path segments, byte-exact vs a hand written tree | 5_rename_rust.rs rust_rename_matches_the_hand_written_after |
| rust | rename: the renamed crate passes `cargo check` | 5_rename_rust.rs renamed_fixture_crate_passes_cargo_check |
| rust | rename: glob importer is a Dynamic stop | 5_rename_rust.rs glob_importer_is_a_dynamic_stop |
| rust | rename: shadowed item at module root needs no `--at` | 5_rename_rust.rs shadowed_items_need_no_at |
| rust | rename: macro body paths rename, locals stay | 5_rename_rust.rs macro_body_paths_rename_and_locals_stay, renamed_macro_fixture_crate_passes_cargo_check |
| rust | rename: multi-row list commit is atomic | 5_rename_rust.rs list_commit_is_atomic_across_rows |
| rust | rename: method call sites outside the anchor file | none |
| rust | rename: field/variant, derive/serde strings | none |
| ts | move: five importer styles rewritten and re-resolved | 41_move_ts.rs every_importer_style_is_rewritten_and_still_resolves_to_the_moved_file |
| ts | move: moved file re-aims its own relative import | 41_move_ts.rs the_moved_file_re_aims_its_relative_import_and_leaves_the_package_alone |
| ts | move: a file naming no moved target is untouched | 41_move_ts.rs a_file_naming_no_moved_target_is_untouched |
| ts | move: path constants in a staying file, dynamic template import, tsconfig references, jest mapper, `.d.ts` twin, `dist/` | none |
| ts | rename: anchor file byte-exact + tsc clean | 4_rename_ts.rs commit_renames_the_anchor_file, tsc_is_clean_on_the_committed_tree |
| ts | rename: ambiguous and `--at` selection | 4_rename_ts.rs shadowed_inner_binding_needs_no_at, nested_only_candidates_stop_then_at_selects |
| ts | rename: runtime seats stop the run | 4_rename_ts.rs dynamic_stop_lists_every_seat |
| ts | rename: importer walk over exported and aliased clauses | 4_rename_ts.rs exported_symbol_renames_every_importer, aliased_import_moves_only_the_imported_seat |
| ts | rename: namespace importer miss is reported by --verify-scip | 4_rename_ts.rs scip_verify_reports_a_missed_seat, scip_verify_agrees_on_the_exports_fixture, scip_verify_never_changes_the_plan |
| ts | rename: text refs report, never write | 4_rename_ts.rs text_refs_reports_the_string_and_the_readme, text_refs_never_writes, without_the_flag_no_text_ref_rows |
| ts | rename: default import clause, `keyof`/mapped-type property names, `.d.ts` twin | none |
| kotlin | move: explicit import respelled across packages | 4_move_kotlin.rs an_explicit_importer_is_respelled_across_packages |
| kotlin | move: moved file's `package` line respelled | 4_move_kotlin.rs the_moved_files_package_line_is_respelled |
| kotlin | move: wildcard importer warned, left alone | 4_move_kotlin.rs a_wildcard_importer_is_warned_and_left_alone |
| kotlin | move: layout/package disagreement is a named error | 4_move_kotlin.rs a_layout_package_disagreement_is_a_named_error |
| kotlin | move: dry run prints every respell | 4_move_kotlin.rs a_dry_run_prints_every_respell_and_touches_nothing |
| kotlin | move: Gradle source sets, `@JvmName`, `Class.forName` strings, `typealias` | none |
| kotlin | rename: decl ident + import trailing segment + FQ + alias clause | 7_rename_kotlin.rs kotlin_rename_matches_the_hand_written_after |
| kotlin | rename: wildcard importer is a Dynamic stop | 7_rename_kotlin.rs wildcard_importer_is_a_dynamic_stop |
| kotlin | rename: same-named decl in another package is out of reach | 7_rename_kotlin.rs shadow_in_other_package_needs_no_at |
| kotlin | rename: extension/member call sites on a value receiver, overloads, local shadow over-reach | none |
| go | move and rename | none (no arm, no test) |
| python | move and rename | none (no arm, no test) |

## Lane 4 gaps vs the rust arm

| rust arm capability (file:line) | go proposal covers | python proposal covers |
|---|---|---|
| rust_rehome.rs:82 directory_stem = "mod"; moved_names adds the directory name for a directory-standing file | no: go has no directory-standing file, the plan says nothing about `_test.go` or the package clause | no: `__init__` is the python directory-standing file; the plan proposes no directory_stem |
| rust_rehome.rs:86-185 import_refs resolves every `mod` decl against rustc's file law before judging any `use` | partial: import specs are read, `internal/` and the package clause are not | partial: clauses are read, relative dots are not recomputed |
| rust_rehome.rs:155-176 a moved file is parsed unconditionally; stayers are prefiltered by carries_name (:1577) | no: no prefilter or cost law stated | no: no prefilter or cost law stated |
| rust_rehome.rs:186-209 respell is batch-aware: importer AND target may both move, one ref per span | partial: an import path segment is rewritten, not a package clause | partial: `import x as z` respell only |
| rust_rehome.rs:199 USE_PATH respell returns None (the tree is held still, `#[path]` is grown instead) | no analogue: nothing plans a re-aim or a hold-still choice | no analogue |
| rust_rehome.rs:211 RehomePlanCheck gate: no parent module stops the run before staging | no: no plan_check proposed | no: no plan_check proposed |
| rust_rehome.rs:38 INCLUDE_MACROS; include_respell :807 | no: `//go:embed` is the analogue and the plan does not mention it | no: `importlib.resources` / data files are not mentioned |
| rust_rehome.rs:217 RehomeManifests: only packages owning a moved file are parsed and written | partial: `go.mod` module line named, `go.work`/vendor not | no manifests leg proposed |
| rust_rehome.rs:1413 manifest_leaves: target tables lib, bin, test, bench, example, plus `[package] build` | no analogue for `go.mod` require/replace or embed patterns | no analogue for entry points, packages list, pytest config |
| rust_rehome.rs:1531 crate_roots reads the manifests because a `[[bin]] path` can put a root anywhere | no analogue: the module root comes from `go.mod` | no analogue: the package root comes from `__init__.py` ancestry |
| rust_rehome.rs:1563 auto_crate_root: target auto-discovery by path shape | no analogue: `_test.go` and `internal/` shape rules exist and are not planned | no analogue: PEP 420 dirs change what is importable |
| rust_rehome.rs:890 build_relocate_plan, :1531 crate_roots: both cached per process | no: no batch/plan caching law stated | no: no batch/plan caching law stated |
| rust_rehome.rs:1202 run_edit, :1038 insert_decls, :1262 qualifier_of: decl lifted, `mod` lines inserted, crate-wide `::` runs rewritten | no: no relocation arm; a go move into package `b` silently keeps `package a` | no: no relocation arm; relative import depth is not recomputed |
| rust_rehome.rs:982 widen_privates: `pub(crate)` promotion keyed to who reaches the item, nothing widened past pub(crate) | no: go exportedness is capitalization, nothing planned | no analogue: python has no visibility keyword |
| types.rs:2763 Respell; rust_rehome.rs:815 manifest_respell threads a receipt; one text-edit vocabulary for messages and manifests | no: no receipt law proposed | no: no receipt law proposed |

## Build-vs-buy

| crate | published | version | license | note |
|---|---|---|---|---|
| ruff_python_resolver | no | none | none | `cargo search ruff_python_resolver --limit 1` prints `ruff = `0.16.8`` (the index has no such name); `cargo info` exits with: could not find `ruff_python_resolver` in registry `https://github.com/rust-lang/crates.io-index` |
| ruff_python_semantic | yes | 0.0.14 | MIT | self-described (cargo info) as an internal component crate of Ruff; 0.0.x, rust-version 1.96, drags the ruff_python_ast/pycross family; it resolves bindings and scopes, it does not resolve import paths to files |
| ra_ap_hir_def | yes | 0.0.352 | MIT OR Apache-2.0 | rust-version 1.98; this crate already pins the ra_ap family at 0.0.349 (Cargo.toml:79-102) and already depends on ra_ap_hir under feature `rust-checker`, which depends on ra_ap_hir_def internally; adding it at 0.0.352 beside 0.0.349 duplicates the family in the lock |

## Verdict on "100 without a compiler"

Shares are rows graded `deterministic` over all rows for that language in the two tables above. `top-level` and `method/field` are the two symbol kinds of the rename table.

| language | move: deterministic share | rename top-level: deterministic share | rename method/field: deterministic share |
|---|---|---|---|
| rust | 8/12 | 5/5 | 0/4 |
| ts | 7/10 | 6/10 | 0/2 |
| go | 8/10 | 3/4 | 0/5 |
| kotlin | 6/9 | 3/5 | 0/4 |
| python | 8/12 | 2/7 | 0/5 |
