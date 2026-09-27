# hafley_scm review, origin/main 30d42aff, 2026-09-27

hafley_scm read-only review at 30d42aff (the working tree is on main at that commit). Nothing was built or run. The review was split across four read-only forks (Rust, TS/JS, Python/Go/Kotlin, cfg/SCIP/macros/.scm) plus a direct read of `deps.rs` and `project.rs`. The `deps.rs` items were checked line by line directly; for fork items, the confidence label is the fork's own and those lines were not re-read. Paths are relative to `/Users/chrishafley/projects/hafley-rs/crates/hafley_scm/`.

## Findings, most severe first

1. **`src/read/lang/8_scm_store.rs:10-37` + `src/read/lang/7_scm_rows.rs:195-204`** — `scm_edges` builds a clique (every root to every root, every import to every root). `RESOLVE_SQL` then enumerates simple paths to depth 64 with `UNION ALL` and a per-path `seen` string, with no global visited set.
   - Failure: 12 files gives about 4.8e8 paths per reference, so it never terminates in practice.
   - Confidence: confirmed.
2. **`src/read/lang/7_scm_rows.rs:195-204`** — the same clique makes every root-owned def visible to every reference, whether or not anything imports it.
   - Failure: `a.rs` calls `helper()`, and `b.rs` and `c.rs` both define `helper` with no imports; edges go to both.
   - Confidence: confirmed.
3. **`src/lang/rust/20_tree_module_specifier_rows.rs:96-113`, `21_tree_module_resolution_rows.rs:185-205`** (the live tree-sitter front-end) — a `scoped_identifier` inside a use-list ignores the accumulated prefix.
   - Failure: `use std::{collections::HashMap}` gives module `collections::HashMap`.
   - Failure: `use crate::{a::B}` loses the `crate` anchor and falls into the corpus suffix search.
   - The syn twin is correct; the parity test (`rust_modules.rs:2660`) covers only 4 fixture dirs.
   - Confidence: confirmed.
4. **`20_…:89`, `21_…:178`** — after a nested `scoped_use_list`, `prefix.clear()` runs instead of truncating back to the entry length.
   - Failure: in `use a::{b::{c,d}, e}`, `e` gets module `e` instead of `a::e`.
   - Confidence: confirmed.
5. **`21_…:236-244` vs `20_…:160-169`** — a glob inside a list is wrong in both walkers, in opposite directions.
   - Failure: `use a::{b::*}` gives star qualifier `[a]` in one walker and module `b` in the other.
   - Confidence: confirmed.
6. **`3_module_specifier_rows.rs:37`, `20_…:38`, `10_…:100`, `21_…:82`** — `mod x;` inside an inline `mod a {}` is emitted with no inline prefix.
   - Failure: `src/lib.rs` containing `mod a { mod b; }` resolves to `src/b.rs`; rustc uses `src/a/b.rs`. This affects both `deps.rs::resolve_rust_module` and `RustModuleIndex::mod_file`.
   - `#[path]` inside an inline module has the same defect.
   - Confidence: confirmed.
7. **`src/read/lang/rust_modules.rs` `home_file` (~2290) + `src/read/lang/rust/2_call.rs:275-310`** — `self`/`super` are derived from the file path, and use rows are flattened across inline modules.
   - Failure: `mod tests { use super::*; }` in `src/foo.rs` globs `lib.rs`'s items into `foo.rs` scope instead of `foo`'s.
   - Confidence: confirmed.
8. **`rust_modules.rs:2507-2548` `export_table`** — only `reexport` uses enter the table, but private `use` items are visible to child modules through `use super::*`.
   - Failure: `tests.rs` with `use super::*` misses the parent's `use crate::x::Bar`, and `Bar` becomes unresolved or a corpus-unique guess.
   - Confidence: confirmed.
9. **`rust_modules.rs:2525-2530`** — a later Call-family def always overwrites an earlier one with the same name, and every def is exported regardless of visibility or nesting (including fn-local items).
   - Failure: `mod a { pub fn f(){} } mod b { pub fn f(){} }` resolves `f` to b's def instead of Ambiguous.
   - Confidence: confirmed.
10. **`src/read/deps.rs:523-570` `nearest_manifest_dir` / `is_cargo_root`** — crate-root detection requires `Cargo.toml` to be inside the supplied path universe. `ryi --deps` universe = supplied paths (`deps.rs:748-756`).
    - Failure: `ryi --deps $(fd -e rs)` makes `src/lib.rs` a non-root, so `mod foo;` probes `src/lib/foo.rs` and returns RelativeUnresolved.
    - Failure: when only a workspace-root `Cargo.toml` is supplied, `crates/x/src/lib.rs` is also a non-root.
    - Confidence: confirmed.
11. **`deps.rs:562-569` `is_cargo_root`** — the auto-discovered multi-file targets `tests/<name>/main.rs`, `examples/<name>/main.rs` and `benches/<name>/main.rs` are missing.
    - Failure: `mod util;` in `tests/it/main.rs` probes `tests/it/main/util.rs` instead of `tests/it/util.rs`.
    - `[lib] path` and `[[bin]] path` are also ignored.
    - Confidence: confirmed.
12. **`rust_modules.rs:657-670` `mod_dir`** — any file named `lib`, `main` or `build`, and any file whose parent is `bin`, `tests`, `examples` or `benches`, is treated as owning its own directory.
    - Failure: `src/tests/foo.rs` with `mod bar;` probes `src/tests/bar.rs` instead of `src/tests/foo/bar.rs`.
    - Failure: `src/cli/main.rs` (a non-root module) is misclassified the same way.
    - These rules disagree with `deps.rs:is_cargo_root` on the same input.
    - Confidence: confirmed.
13. **`src/read/lang/rust/2_call.rs:237-251` `module_segments`** — drops every `src` segment anywhere in the path, maps `-` to `_`, and pops a trailing `lib`/`main`/`mod`.
    - Failure: `a/src/b.rs` collides with `b`.
    - Failure: `src/foo/main.rs` (module `foo::main`) is keyed as `foo`.
    - Confidence: confirmed.
14. **`rust_modules.rs` `home_file` crate branch (~2310-2325)** — `crate::` anchors to the package dir, which matches only `src/lib.rs` and `src/main.rs`.
    - Failure: in `tests/it.rs` with `mod util;`, `crate::util::f` looks up `src/util.rs`.
    - Confidence: confirmed.
15. **`rust_modules.rs:1169-1236, 627-649`** — dependency renames are not mapped.
    - Failure: `foo = { package = "bar", path = "../bar" }` means source `foo::x` never reaches lib `bar`.
    - `[target.'cfg(..)'.dependencies]` is not read.
    - Confidence: confirmed.
16. **`rust_modules.rs:1286-1288`** — `known_crate_idents` comes from the directory name, not the package or lib name.
    - Failure: `crates/core` with `name="hafley-core"` means `use hafley_core::X` is not recognized, while `use core::…` (std) is treated as a corpus crate.
    - Confidence: confirmed.
17. **`rust_modules.rs:971-1043`** — Cargo target discovery is hard-coded to the default layout.
    - `[[bin]]`/`[[test]]` `path =`, `autobins = false`, and a `[lib] path` outside `src/` are ignored.
    - Files reached only through `[[bin]] path` get no scope.
    - Confidence: confirmed.
18. **Rust `extern crate`, `::`, `$crate` (grep: only `2_call_metadata_rows.rs:251` touches `ItemExternCrate`)** — these forms are not handled.
    - Failure: `extern crate foo as bar; bar::f()` does not resolve.
    - Failure: leading `::` is stripped, so `::foo::x` binds to a local module `foo`.
    - Confidence: confirmed.
19. **`rust_modules.rs:1579-1592` `trait_in_scope`** — `use path::Trait as _;` stores local `_`, so the trait never counts as in scope.
    - Failure: `x.method()` whose only candidate is that trait's impl stays unbound.
    - The check also matches a `use` anywhere in the file.
    - Confidence: confirmed.
20. **`rust_modules.rs:684-711` `PRELUDE_TRAITS`** — the list is the 2015/2018 prelude and no edition is read. The 2021 additions `TryFrom`/`TryInto`/`FromIterator` and the 2024 additions `Future`/`IntoFuture` are missing.
    - Failure: `x.try_into()` in a 2021 crate is filtered out of `impl_target`.
    - Confidence: confirmed.
21. **`src/read/lang/rust/1_type.rs:287`** — the prelude-shadow guard lists only `Result` and `Box`.
    - Failure: a bare `Option`/`Vec`/`String` binds to a corpus-unique `struct String` in an unrelated file.
    - Confidence: confirmed.
22. **`22_tree_receiver_rows.rs:522-536`, `21_…:283-296`** — `Option<T>` and `Result<T>` collapse to `T`, both for receiver typing and for impl self types.
    - Failure: `let x: Option<Foo>; x.map()` binds to `Foo::map`.
    - Failure: `impl Trait for Option<Foo>` is indexed as an impl on `Foo`.
    - Confidence: confirmed.
23. **`src/read/lang/rust/2_call.rs:209-226` `module_qualifier`** — whether a path head is a type or a module is decided by uppercase-first.
    - Failure: `u32::from_str_radix`, `str::from_utf8` and `i64::MAX` are suffix-matched against a corpus `u32.rs` or `str.rs`.
    - Confidence: confirmed.
24. **Raw identifiers (`20_…:41`, `21_…:81`, `3_…:40`)** — `r#` is never stripped.
    - Failure: `mod r#type;` probes `r#type.rs`; rustc uses `type.rs`.
    - Confidence: confirmed.
25. **`20_…:219-227`, `21_…:327-351`, `3_…:141` `path_attribute`** — the attribute is parsed by string split.
    - `#[cfg_attr(windows, path="win.rs")]` is ignored.
    - `#[path = r"x.rs"]` degrades to a plain `Module` row.
    - Confidence: confirmed.
26. **`20_…:37-40`** — `pending_attrs` is not cleared after an inline `mod` body, and a comment node between an attribute and its `mod` clears the pending attributes.
    - Failure: `#[path="p.rs"] mod inline {}` followed by `mod foo;` gives `foo` the path `p.rs`.
    - Confidence: confirmed.
27. **`20_…:115-124`** — `use foo::{self as bar}` is dropped: `path_segments` is empty after popping `self`, so the function returns early.
    - Confidence: confirmed.
28. **`src/read/lang/ts_resolve.rs:1168-1175, 1223`** — an explicit `export {x} from` whose target is unresolved does `continue`, and the star pass then fills `x` with `or_insert`.
    - Failure: `export {x} from 'ext'; export * from './local'` binds `x` to `./local`. ECMA-262 says the explicit export shadows the star.
    - Confidence: confirmed.
29. **`src/read/lang/ts.rs:4666-4700` `call_name_match`** — a free callee binds to the single corpus file that declares that name, with no scope or import evidence. The same-file step takes the first def with that name.
    - Failure: global `fetch()` or `describe()` binds to a corpus `fetch`/`describe`.
    - Failure: a class method `load` beats a top-level `function load` for a bare `load()` call.
    - Confidence: confirmed.
30. **`src/read/lang/ts_receivers.rs:244-250, 292-313, 459-493`** — scope frames exist only for function and arrow bodies. Destructured names, array patterns, blocks, loops, catch clauses, classes and nested function declarations never shadow.
    - Failure: `function Btn({onClick}) { onClick() }` binds to an unrelated corpus `onClick`.
    - Failure: sibling-block `const r` declarations merge to Ambiguous.
    - Confidence: confirmed.
31. **`ts_resolve.rs:99-100`** — the oxc_resolver options are `main_fields ["module","main"]` and `condition_names ["node","import"]`. There is no `types` field or condition, and no `require` condition.
    - Failure: a workspace package with `"types":"src/index.ts","main":"dist/index.js"` resolves to `dist/`, which is outside the corpus, so no edge.
    - Failure: `require('pkg')` uses the `import` condition.
    - Confidence: confirmed.
32. **`ts_resolve.rs:918-950`** — only the `link:` protocol is recognized. `workspace:*`, `file:`, npm/yarn `workspaces` globs and `pnpm-workspace.yaml` are not.
    - The `link:` fallback joins the subpath onto the package root and bypasses `exports`.
    - Failure: `@s/pkg/sub` resolves to `<root>/sub` instead of `exports["./sub"]`.
    - Confidence: confirmed.
33. **`deps.rs:173-194` `TsconfigPaths::rewrite`** — `paths` patterns are tried in BTreeMap (lexicographic) order and the first hit wins. TypeScript uses the longest-prefix match.
    - Failure: `{"*":["types/*"],"@app/*":["src/*"]}` tries `types/@app/x` first, and if that file exists it wins over `src/x`.
    - Confidence: confirmed.
34. **`deps.rs:138-143`** — only `<root>/tsconfig.json` is read. `extends`, nested per-package tsconfigs, `references`, `rootDirs` and package.json `imports` are ignored.
    - Failure: `paths` declared in `tsconfig.base.json` means every alias becomes NodeModulesBoundary.
    - Failure: `#internal/x` is classified as a package.
    - Confidence: confirmed.
35. **`deps.rs:361-373` `normalize`** — a `..` above the root is clamped (popped from an empty stack). The doc at 350-351 says such a specifier becomes unresolvable, but it can resolve.
    - Failure: from `a.ts`, `import '../b'` with `b.ts` in the universe resolves to `b.ts`.
    - Confidence: confirmed.
36. **`deps.rs:318-321` vs `ts_resolve.rs:35-40`** — the exact match is checked before the emitted-name rewrite.
    - Failure: when both `x.js` and `x.ts` exist, diet picks `x.js` and the oxc path and tsc pick `x.ts`. The two resolvers emit different edges.
    - Confidence: confirmed.
37. **`deps.rs:221, 253, 278`** — `strip_json_extras` pushes `byte as char`, which garbles non-ASCII UTF-8. `drop_trailing_commas:267` detects escapes by looking only at the previous byte.
    - Failure: a `paths` key `"@ü/*"` never matches.
    - Failure: a string ending in `\\"` flips the in-string state.
    - Confidence: confirmed.
38. **`ts_resolve.rs:140-163`** — `.ts` precedes `.d.ts` in `EXTENSIONS`, so `.d.ts` is never recognized as an extension.
    - Failure: `respell("../y.d.ts", "./x.js")` gives `../y.d.js`.
    - Confidence: confirmed.
39. **`deps.rs:58-70`, `ts_resolve.rs:30-40`** — `.d.mts` and `.d.cts` are missing from both extension lists.
    - Failure: `import './x.mjs'` with only `x.d.mts` present is unresolved.
    - Confidence: confirmed.
40. **CommonJS (`ts_resolve.rs:590-606, 663-683`)** — CommonJS produces a file edge only.
    - `const {a} = require()` binds no names.
    - `module.exports` and `exports.a` produce no exports.
    - `export = {…}` is dropped.
    - Confidence: confirmed.
41. **`src/read/lang/go.rs:3088-3101` `go_module_of`** — go.mod is parsed with `strip_prefix("module ")`.
    - Failure: `module example.com/m // c` or a quoted module path makes every in-module import external.
    - Failure: `module\texample.com/m` returns None.
    - Confidence: confirmed.
42. **`go.rs:2905, 3665, 3912, 4160`, `go_modules.rs:253, 297, 458, 488`** — `go_module_of` is uncached and does `read_to_string` of go.mod at every ancestor directory, once per candidate, embed or binding call.
    - Confidence: confirmed.
43. **`go.rs:710-730, 3557-3576` vs `go_modules.rs:456`** — the import qualifier is the last path segment in two places and the directory's real package name in the third.
    - Failure: a dir `go-utils` with `package utils`, or a `/v2` path, makes the `utils.X` selector miss in the embed, type-id and call legs.
    - Confidence: confirmed.
44. **`go_modules.rs:436-452` `sites_in_dir`** — no filtering of `_test.go` files, `foo_test` packages, build tags or GOOS/GOARCH suffixes.
    - Failure: `Open` in both `file_unix.go` and `file_windows.go` is ambiguous and unresolved.
    - Failure: a `pkg.NewFixture` defined only in a test file is bound from non-test importers.
    - Confidence: confirmed.
45. **`go_modules.rs:215-236`** — the first package clause seen wins the directory.
    - Failure: a `//go:build ignore` `package main` generator file relabels `pkg/` as `main`.
    - Confidence: plausible.
46. **Go multi-module layouts (`go.rs:3106`)** — no go.work, no `replace` (although `manifests.rs` reads `Replacement::FilePath`), and no vendor/.
    - Failure: an import of a sibling module present in the corpus is classified external.
    - Confidence: confirmed.
47. **`src/read/lang/python/_2_modules.rs:93-114`** — `import a.b` records `local="a.b"`; Python binds `a`.
    - Failure: `import pkg.sub; pkg.sub.f()` never matches.
    - Confidence: confirmed.
48. **`python/_2_modules.rs:282-298` `module_name`** — a file in a directory without `__init__.py` gets its bare stem as its absolute module name.
    - Failure: `tools/json.py` captures `import json` corpus-wide.
    - Failure: two `utils.py` files make `import utils` ambiguous everywhere.
    - Failure: PEP 420 `ns/pkg/mod.py` is named `mod`.
    - Confidence: confirmed.
49. **`python/_2_modules.rs:307-315`** — when both `foo.py` and `foo/__init__.py` exist, absolute resolution returns None, while relative resolution prefers the package.
    - Confidence: confirmed.
50. **`python/_2_modules.rs:388-446`** — binding precedence and star handling are wrong:
    - top-level defs always win over imports;
    - the first import wins, where Python's last rebinding wins;
    - `?` at line 396 aborts the whole lookup on the first unresolved module;
    - star imports ignore `__all__` and `_private` names.
    - Failure: `from a import X` then `from b import X` binds `a.X`.
    - Confidence: confirmed.
51. **`src/read/lang/kotlin_modules.rs:371-388`** — import precedence is textual order, where Kotlin gives explicit imports priority over star imports.
    - Failure: `import x.*` then `import y.Foo` binds `x.Foo`.
    - Confidence: confirmed.
52. **`kotlin_modules.rs:391-398`** — a name declared in two files of one package counts as ambiguous. This is legal for overloads, extensions and expect/actual.
    - Failure: `fun String.fmt()` in `A.kt` and `fun Int.fmt()` in `B.kt` mean `import u.fmt` binds nothing.
    - Confidence: confirmed.
53. **`src/read/lang/kotlin_receivers.rs:430-441, 496-502`** — bare calls and `this.` are typed as the innermost class; lambdas with a receiver are ignored.
    - Failure: `buildString { append("x") }` inside class `A` types `append` as `A`.
    - Confidence: confirmed.
54. **`src/read/cfg.rs:91, 136`** — Rust `match` and TS `switch` use `Fixed(Branch)` over a wrapper node (`match_block`/`switch_body`), so the arms are chained in sequence.
    - Failure: `match x {A=>f(), B=>g()}` produces the path f→g.
    - Confidence: confirmed.
55. **`cfg.rs:676-694, 87-108, 107/147/179`** — control flow is wrong for jumps and yields:
    - `break`/`continue` labels are ignored;
    - `switch`/`select` are not break targets;
    - `?` and `&&`/`||`/`??` have no role;
    - `yield` maps to Exit.
    - Failure: `'outer: loop { loop { break 'outer } }` exits only the inner loop.
    - Failure: in `f()?; g()`, `g` always runs.
    - Failure: code after `yield` is unreachable.
    - Confidence: confirmed.
56. **`cfg.rs:115-118, 696-723, 813-869`** — Go if/switch/select structure, loop bodies and try/finally are modeled wrong.
    - Go `if init; cond` takes the init as the condition.
    - A tagless `switch` or `select` swallows its first case as the condition.
    - Python `for…else` takes the `else` as the loop body.
    - `loop {}` gets a fall-through edge.
    - `finally` is linked as a sibling handler arm, so `return` skips it.
    - Confidence: confirmed.
57. **`src/pipeline/run_over_file_tree/test_predicates_per_candidate.rs:33`** — `text.windows(literal.len())` panics on an empty literal.
    - Failure: `(#contains? @x "")` panics.
    - Confidence: confirmed.
58. **UTF-8 `expect` on source bytes: `16_macro_invocation_rows.rs:185, 230`, `21_…:376`, `src/read/lang/6_scm_family.rs:39-94`, `kotlin.rs:681, 688, 738`** — these panic on non-UTF-8 input. `20_…:235` uses `unwrap_or_default` for the same case.
    - Confidence: plausible (depends on whether callers check UTF-8 first).
59. **`src/lang/rust/2_call_metadata_rows.rs:19-25`, `16_…:225-235`** — the proc-macro2 column (counted in chars) is added to a byte line-start.
    - Failure: any non-ASCII character earlier on the line shifts macro ranges, so the SCIP containment checks in `rust_scip_macros.rs:193` and `14_…` miss.
    - Confidence: confirmed.
60. **`src/lang/rust/syn_macro_expansion_defs.rs`** — macro scoping and expansion are wrong:
    - lines 147-166, 400-406: `macro_rules!` definitions are held in a file-wide HashMap where the last definition wins, and invocations match on the last path segment only;
    - lines 203-205, 428-437: expansion reparses space-joined tokens, which loses the `$e:expr` grouping and hygiene.
    - Failure: two `m!` defined in different inline modules both expand as the last one.
    - Failure: `dbl!(1+f())` becomes `2 * 1 + f()`.
    - Confidence: first confirmed, second plausible.
61. **SCIP symbol handling in `src/read/scip_v5_rels.rs`:**
    - 520-535 `descriptor_name` takes the last ASCII identifier run, so `Foo#bar(+1).` gives the name `1` and backtick-escaped names are cut.
    - 409-415 `enclosing_fn` is a backward linear scan that ignores the decoded `enclosing_range`, so references after a nested fn are attributed to the inner fn.
    - 447-462 the `use` check scans back to the last `;` or `}`, so `d` in `use a::{b::{c}, d}` becomes a call edge.
    - 299-311 splitting on single spaces breaks on the escaped double space.
    - Confidence: 520-535, 409-415 and 447-462 confirmed; 299-311 plausible.
62. **`src/read/lang/rust_scip_macros.rs:136-138`** — the SCIP document is matched to a file by content blob.
    - Failure: two byte-identical files both take the first document's occurrences.
    - Confidence: confirmed.
63. **`src/lang/rust/4_fast_query.scm:44-47, 69-73`** — the query is too narrow for bindings and too broad for calls.
    - Only `let`/parameter identifier patterns bind. No tuple/struct patterns, `if let`, match arms, `for` patterns or closure parameters.
    - Method calls `x.foo()` are captured as lexical `@local.call`.
    - Failure: `for f in fs { f() }` resolves to an outer `fn f`.
    - Failure: `self.len()` resolves to a same-file `fn len`.
    - Confidence: confirmed.
64. **`crates/sprefa-extract/queries/kotlin/scip.scm:139-141`** — `(infix_expression (simple_identifier) @site.callee)` has no anchor or field, so every direct child matches.
    - Failure: `a to b` records callee `b`.
    - Confidence: confirmed.
65. **Quadratic scans, one per bullet:**
    - `src/read/project.rs:2742-2760`: per type candidate, `types.nodes.iter().position` plus `resolved.iter().any`, so O(candidates×nodes) per file.
    - `ts.rs:4926-4929` and `rust_scip_macros.rs:136`: linear `position` over SCIP documents per file, so O(files²).
    - `ts_resolve.rs:1323-1345` `def_at`: O(imports×defs).
    - `ts_resolve.rs:1227-1232`: cyclic re-export tables are never cached.
    - `rust_modules.rs:2363-2376` `exact_module`: O(files) per lookup, uncached.
    - `syn_macro_expansion_defs.rs:635-715`: chunk scans per edit, up to 8 passes.
    - `7_scm_rows.rs:325-356`: linear copies of `Nest::innermost`.
    - `python/_2_modules.rs:472-529`: star chains are walked with no memo.
    - Confidence: confirmed by reading; the runtime impact of the `ts_resolve.rs:1227` cycle-caching case is plausible.
66. **Unbounded process-global caches** — `ts_receivers.rs:627` `facts_cache`, `go.rs:1158, 3195, 3757`, `kotlin_receivers.rs:67`: no eviction or clear.
    - Failure: in a long-running serve, path-keyed Go facts go stale.
    - Confidence: confirmed that no clear exists.
67. **`src/read/scip_ensure.rs:775-870`** — the deadline applies per `run_capped` call, not per build, and processes left in the group are not killed on success.
    - The comment at 933-935 says the crate has no libc dependency; Cargo.toml has an optional `libc`.
    - Confidence: plausible.

**Duplicated code with divergent rules:**
- Rust use-tree walk ×4 (`3_`, `10_`, `20_`, `21_`).
- Rust `mod`→file ladder ×3 (`deps.rs:486-570`, `rust_modules.rs:657/1763`, the `crate_dirs_of` loops).
- `principal_ty` ×3.
- TS resolver ×2 (`deps.rs` hand-written, `ts_resolve.rs` oxc).
- `require`/`import()` detection ×3 (`ts_resolve.rs:590`, `ts.rs:2240`, `ts.rs:2876`).
- Python import walker ×2.
- Go import-qualifier logic ×3.
- `unique_blob` ×3.
- `BUILTIN_MEMBERS` (`ts.rs:4771-4772`) lists `"fill"` twice.
- `oxc_semantic` is a dependency under the `typescript` feature with 0 uses in `src`.
- `hafley_scm` `include_str!`s `../../../../sprefa-extract/queries/*.scm` (`kotlin.rs:56`, `ts.rs:59`), so the crate is not self-contained.

**Name-text special cases:**
- `cfg.rs:96, 100, 125, 545` match `catch_unwind`, `panic|todo|unreachable` and Go `panic`/`recover` by callee text; `unimplemented!` is missing.
- `ts_paths.rs:19, 110-120` `PATH_CALLEES` treats any `X.resolve`/`X.join` as a path call, so `Promise.resolve('./x')` gets rewritten.
- `python/_1_type_edges.rs:36-43` `TUPLE_HEADS` matches `typing.Tuple` only.
- `kotlin_receivers.rs:378-400` treats an uppercase callee as a constructor.

## Overfitting / reinvention

| File | Hand-rolled logic | Crate |
|---|---|---|
| `src/read/deps.rs:118-346` | tsconfig JSONC parse, `paths`/`baseUrl`, extension/index probing | `oxc_resolver` (already a dependency; its `FileSystem` trait can back it with the in-memory universe); `jsonc-parser` / `json_comments` for the JSONC part alone |
| `src/read/deps.rs:486-570`, `src/read/lang/rust_modules.rs:575-1236` | Cargo.toml parsing, target discovery, renames, workspace inheritance | `cargo_metadata`, `cargo_toml` / `cargo-util-schemas`, `ra_ap_project_model` (already optional) |
| `rust_modules.rs` (`home_file`, `export_table`, `star_contributions`), `src/read/lang/rust/2_call.rs:209-340`, `PRELUDE_TRAITS`, `1_type.rs:287` | module tree, `self`/`super`/`crate`, globs, re-exports, visibility, edition-aware prelude, type-vs-module heads | `ra_ap_hir_def` DefMap/nameres via `ra_ap_hir` (already optional under `rust-checker`) |
| `20_`/`21_` `path_attribute`, `3_ mod_path_attr` | `#[path]` / `cfg_attr` parsing | `syn::Attribute::parse_nested_meta`; `ra_ap_syntax` `ast::Attr` |
| `22_tree_receiver_rows.rs` | local type inference (Option/Result unwrap, `new` returns Self) | `ra_ap_hir` inference (wired in `rust_checker_ra.rs`) |
| `syn_macro_expansion_defs.rs` | `macro_rules` scoping and expansion | `ra_ap_hir_expand` / `ra_ap_hir_def`; `ra_ap_syntax_bridge::prettify_macro_expansion` |
| `2_call_metadata_rows.rs:19`, `16_…` `syn_compatible_byte` | line/col→byte conversion | `proc_macro2::Span::byte_range()` (span-locations already enabled) |
| `ts_receivers.rs:85-105, 459-620` | scope stack and shadowing | `oxc_semantic` `Scoping` (already a dependency, unused) |
| `ts.rs:153` `source_type_for` | extension→SourceType mapping | `oxc_span::SourceType::from_path` |
| `ts_resolve.rs:903-950` | workspace/`link:` discovery | `oxc_resolver` `alias`/`roots` fed from workspace manifests; `globset` for workspace globs |
| `src/read/cfg.rs` | CFG over the CST | `oxc_cfg` / `oxc_semantic` ControlFlowGraph (TS/JS); `ra_ap_hir` body (Rust) |
| `src/read/scip/scip_proto.rs` (1780 lines), `scip_v5_rels.rs` symbol parsing | SCIP protobuf bindings, symbol grammar | `scip` crate (`scip::symbol::parse_symbol`) |
| `7_scm_rows.rs` + `8_scm_store.rs` | scope graph in a recursive SQLite CTE | `tree-sitter-stack-graphs` + `stack-graphs` |
| `go.rs:3088` `go_module_of` | go.mod parsing | `gomod-parser` (already a dependency, used in `manifests.rs:209`) |
| `go_modules.rs`, `go.rs` package/build-tag logic | build constraints, `_test`, GOOS | no full Rust crate found; the reference is `go list -json` / `golang.org/x/tools/go/packages` |
| `python/_2_modules.rs` | module naming, relative/absolute imports, star, `__all__` | `ruff_python_resolver`, `ruff_python_semantic`, `ruff_python_parser` |
| `test_predicates_per_candidate.rs:33` | substring search | `memchr::memmem` (already a dependency) |
| `scip_ensure.rs` `run_capped` / `kill_process_group` | process-group timeout and kill | `process-wrap` or `wait-timeout` |
| `scip.rs` `cargo_manifests_below` / `manifest_uses_parent_path` | workspace and path-dependency discovery | `cargo_metadata` |
