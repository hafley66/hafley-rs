# scm++ -> SQL -> TypeSpec scope (2026-10-01)

Read-only scoping. Paths are relative to the worktree root `.boop-worktrees/feature/writer-ds2`. Counts come from `crates/sprefa-extract/bench/boop-effects.db` (484 MB, 286 `file` rows, 129 of them under `*/tests/*`). No code was edited and no build was run.

## 1. Inventory

### 1a. scm++ operators (engine `crates/hafley_scm`)

The engine is `build(&Language, &str) -> Result<QueryExt, QueryExtError>` and `run(&QueryExt, path, src, &Tree, limit, &mut MatchArena)` at `src/lib.rs:22,50`. tree-sitter evaluates its own text predicates. hafley_scm evaluates every operator that `general_predicates` returns, and rejects any operator it does not recognize (`parse_into_predicate.rs:119`).

| operator | args | walk / semantics | source |
|---|---|---|---|
| `#has?` / `#not-has?` | `@c kind+ [neighbor\|end]` | strict descendant (`end`) or direct child (`neighbor`), named and anonymous | `split_predicates_into_kind_queries/parse_into_predicate.rs:57-117`, `walk/ts_descendant_holds.rs` |
| `#has-ancestor?` / `#not-…` | `@c kind+ [neighbor\|end]` | strict ancestor via `Node::parent()` loop | `walk/ts_ancestor_holds.rs` |
| `#has-parent?` / `#not-…` | `@c kind+` | direct parent | `walk/dispatch_by_direction.rs:9` |
| `#contains?` / `#not-…` | `@c "lit"+` | every literal is a byte substring of the capture | `run_over_file_tree/test_predicates_per_candidate.rs:28-37` |
| `#emit!` | `"relation" ("key" @cap\|"lit")+` | one `EmittedFact` row per match | `ts_read_general_predicates.rs:73-115`, `types/0_emit_spec.rs`, `types/1_emitted_fact.rs` |
| native | `#eq? #match? #any-of?` (+`not-`), `#set!`, fields, anchors, quantifiers | evaluated by tree-sitter 0.27 | `crates/hafley_scm/Cargo.toml:7` |

Kind names are validated against the grammar with `id_for_node_kind(kind, true)`, and an unknown name is an error (`parse_into_predicate.rs:87`). These walks are listed as TODO and not built: `Precedes`, `Follows`, `NthChild`, `ByteRange`, and `stopBy: rule` (`types/walk.rs:4-5`, `types/stop.rs:4`).

**Shipped `.scm` files, by language.** Each count is a `grep -oE '#[a-z!?-]+'` over that file.

| file | lang | lines | operators |
|---|---|---|---|
| `crates/hafley_scm/src/lang/rust/4_fast_query.scm` | rust | 89 | none |
| `crates/hafley_scm/src/read/lang/8a_rust_scope.scm` | rust | 14 | 5 `#emit!` (scope, binding, reference) |
| `crates/hafley_scm/src/read/lang/8b_ts_scope.scm` | ts, tsx (same text, `8c_scope_products.rs:57-59`) | 17 | 6 `#emit!` (scope, binding, reference, write) |
| `crates/sprefa-extract/queries/kotlin/scip.scm` | kotlin | 209 | 40 `#emit!` (call.def 5, call.scope 3, call.site 32), 12 `#set!`, 1 `#has-ancestor?` (:122) |
| `crates/sprefa-extract/queries/typescript/scip.scm` | ts | 70 | none |
| `crates/sprefa-extract/gate/*.scm` (10 files) | rust | 5-18 each | `#eq?` 4, `#match?` 16 |

`#has?`, `#has-parent?`, and `#contains?` appear in 0 shipped `.scm` files. They appear in 18 lines under the two crates' `tests/` directories. `#emit!` schemas become Rust accessors in exactly one place: Kotlin, through `crates/sprefa-extract/build.rs:29-77`, which generates `read/lang/kotlin_scm_generated.rs`. No `#emit!` relation reaches SQLite. `ryii query --sqlite` writes only `capture` rows (`crates/sprefa-extract/src/0_query.rs:41-53`).

**"Lang editions": what exists, by type.**
- **Languages.** `RyiLang` has 15 variants (`read/lang/extract_lang.rs:16-35`). `ryii capabilities` prints 12 rows with their planes; for example, rust has `cst,types,call,df` with the rust-analyzer checker, and gdscript and commonlisp have `cst` only.
- **Grammar versions.** These pin the cst kind vocabulary. The pins are in `crates/hafley_scm/Cargo.toml:30-44`: tree-sitter-rust 0.24, typescript 0.23, javascript 0.25, go 0.25, python 0.25, kotlin-sg 0.4, prolog 0.1, json 0.24, yaml 0.7, toml-ng 0.7, md 0.5, html 0.23, gdscript 6, commonlisp 0.4. Cargo features select grammars per language (`Cargo.toml` `[features]`).
- **Rust edition.** `cargo_metadata.rs:16-21,72-73` reads it per target and passes it to rust-analyzer (`7a_rust_checker_project.rs:100`). The sysroot crates are fixed at `"2024"` (`:146,153`). No `.scm` file, cst kind set, or prelude list branches on edition (prelude: review item 20 in `plans/2026-09-27-review-scm.md`).
- **Not found.** No per-edition `.scm` variants exist. The only per-language `.scm` variant split is rust/ts scope and kotlin/ts scip.

### 1b. SQLite fact tables (boop-effects.db)

All ryii record tables come from `crates/sprefa-extract/schema/1_facts.tsp`, which has 74 `record:` models. The generated DDL is `schema/generated/4_facts.sql`, included at `src/bin/ryi/0_sqlite.rs:16`. The rusqlite writers are `generated/7_writers_auto.rs`, included at `0_sqlite.rs:24`. The prose contract `hafley_scm/src/read/schema.rs:17-80` duplicates the TypeSpec models, and it names the Rust `FlatFact` as source of truth.

| table | rows | meaning | key columns | declared in |
|---|---|---|---|---|
| node | 978,633 | one syntax unit per family (family counts: cst 826,969 with 191 named kinds; call 7,886; df 138,816; type 4,962) | `_input_path, family, kind, name, span__start/end` | TSP |
| arg | 58,482 | call argument span by position | `call, pos, arg` | TSP |
| param | 6,729 | parameter span by position | `span, pos` | TSP |
| sig | 7,706 | signature slot type | `owner, slot, pos, ty` | TSP |
| df_field | 5,729 | field write in an owner | `owner, name, value` | TSP |
| df_lit | 8,913 | literal text | `node, kind, text` | TSP |
| df_loop | 915 | loop with var/collection | `span, var, collection` | TSP |
| df_nest | 6,536 | call nested in a loop | `call, loop, depth` | TSP |
| df_allocates | 1,699 | owner that allocates | `owner` | TSP |
| specifier | 2,790 | import/use specifier | `span, name, kind, module, imported` | TSP |
| resolved_import | 1,687 | specifier bound to a target file | `src_path, name, target_path, hops` | TSP |
| method_owner | 1,362 | method's self type / trait | `owner, self_type, trait` | TSP |
| macro_site | 20 | macro invocation | `span, macro_name, source` | TSP |
| doc | 1,494 | doc comment on an owner | `owner, text` | TSP |
| doc_node | 705 | markdown node | `span, kind, name` | TSP |
| const | 157 | const string initializer | `owner, text, kind` | TSP |
| data_doc | 896 | data file document | `ordinal, format, doc` | TSP |
| data_value | 40,543 | data file value by dotted path | `path, kind, text, span` | TSP |
| scip_*, graph_*, symbol, occurrence, local, free_name, capture, fact, witness, run, coverage (each table) | 0 | not requested in this run | | TSP |
| view `callers` | n/a | resolved_edge by callee with grade | | **Rust only** (`0_sqlite.rs:136`) |
| view `uses` | n/a | resolved_type_edge by type | | **Rust only** (`0_sqlite.rs:139`) |
| view `reach` | n/a | recursive closure, depth cap 32 | | **Rust only** (`0_sqlite.rs:143`) |
| view `type_evidence` | n/a | tsi.type facts with witness/run | | **Rust only** (`0_sqlite.rs:145`) |
| view `span_lines` | n/a | every span column with its line via line_start | | **Rust only** (`0_sqlite.rs:56-95`) |
| effect_rule | 58 | callee LIKE pattern -> effect (18 effects) | `pattern, effect` | **hand SQL** `0_effects.sql:5-29` |
| effect_site | 2,293 | external call matching a rule, outside test | `crate, path, line, owner, effect, callee` | **hand SQL** `0_effects.sql:35-62` |
| effect_crate | 54 | effect counts per crate | `crate, effect, sites, owners, files` | **hand SQL** `0_effects.sql:65-67` |
| fn_effect | 5,906 | effects reached over resolved_edge | `path, name, effect` | **hand SQL** `0_effects.sql:72-80` |
| unit | 4,387 | function/method/lambda outside test | `path, start, end, kind, name` | **hand SQL** `1_mutation.sql:7-15` |
| mutation_site | 3,011 | marker: 2,878 cst kind rows plus 133 interior callee rows | `path, start, marker` | **hand SQL** `1_mutation.sql:18-32` |
| unit_direct | 4,387 | unit with marker and effect counts (628 with markers; 791 with effects) | `markers, effects` | **hand SQL** `1_mutation.sql:37-47` |
| impure | 2,710 | unit reaching a marker or effect | `path, name` | **hand SQL** `1_mutation.sql:51-59` |
| pure_unit | 2,813 | unit_direct with neither, not impure | | **hand SQL** `1_mutation.sql:62-65` |
| edge | 955,322 | cst `child` 826,767 (parent span -> child span, `from_kind/to_kind` empty); df `direct` 128,555 | `family, kind, from__*, to__*` | TSP |
| cfg_scope | 1,439 | rust item under `#[cfg(..)]`; all 1,439 rows are `cfg='test'`; 14 of them nest inside another | `span, cfg` | TSP (producer rust only: `lang/rust/2_call.rs:1073`) |
| test_only_call | 1,626 | callee whose every in-file site is under a test cfg | `callee, cfg` | TSP |
| unresolved | 33,313 | call not bound to a corpus def; `external` 32,500 has the checker's crate-qualified path in `detail` | `path, span, reason, detail` | TSP |
| resolved_edge | 11,877 | caller -> corpus callee (checker 11,725) | `caller_path/name, callee_path/name, *_start/_end, resolution_origin` | TSP |
| site | 44,356 | syntactic call site | `span, callee, callee_path` | TSP |
| file | 286 | path, digest, bytes, lines | `path, digest` | TSP |
| line_start | 286 | line offsets per digest (JSON array) | `path, digest, offsets` | TSP |

Facts that are missing for generic policy:
- **Path to package.** `0_effects.sql:49` derives it with `substr(path, 8, …)`. `RustTarget` (`cargo_metadata.rs:18-22`) holds the manifest and edition but no package name or target kind. `package_edge` has 0 rows in this run.
- **Path to language.** `file` has no `lang` column.
- **Grammar kind table.** The engine validates kinds against the grammar, but SQL rules cannot.

## 2. scm++ construct -> SQL equal

Base: `node n` with `family='cst'`, `edge e` with `family='cst' and kind='child'`, joined on `(_input_path, start, end)`. The span join is ambiguous in 7,733 span groups (15,486 nodes share a span), and 7,753 child edges have parent span = child span.

| construct | SQL over fact tables | status |
|---|---|---|
| `(kind) @c` | `node where family='cst' and kind=?` | equal |
| `(p (c))` named child | `edge` child join on spans + kinds | equal, but span-ambiguous on 7,733 groups |
| anchor `.` (named-only) | no child of `p` with span between the two | equal (cst rows are named-only, 0 anonymous kinds) |
| field `name:`, negated field `!f` | edge has no field label; `node.name` = text of the `name` field only (`hafley_scm/src/cst.rs:65`) | **no equal** (except `name:`) |
| anonymous tokens (`"mut"`, `"+="`) | not stored | **no equal** |
| alternation `[a b]` | `kind in (…)` / `union all` | equal |
| quantifiers `* + ?` | multi-row capture grouping | **no direct equal** |
| `#eq? / #any-of?` | `node.name = ? / in (…)` for kinds that carry a name (223,645 cst rows); otherwise `substr(readfile(path),…)` (sqlite3 shell only) | partial |
| `#match?` | `REGEXP` (sqlite3 3.43.2 shell has it; rusqlite needs a registered function) | partial (same text caveat) |
| `#contains?` | `instr(text, lit) > 0` for each literal | partial (same text caveat) |
| `#has-parent?` | `exists edge child where to=c and from kind in (…)` | equal |
| `#has-ancestor? … end` | recursive CTE up `edge`; or span containment `a.start<=c.start and a.end>=c.end and a._row<>c._row` (same-span ties differ) | equal (CTE) / approx (containment) |
| `#has? … neighbor/end` | reverse of the two above | equal (CTE) |
| `#set!` | constant column | equal |
| `#emit! "r" k @c …` | `insert into r(k,…) select …` | equal; **no writer exists** |

Our SQL additions, checked in the other direction:

| addition | scm++ equal |
|---|---|
| test post-filter on `cfg_scope` spans (`0_effects.sql:43-46`, `1_mutation.sql:12-15`) | **none**: `attribute_item` is a preceding sibling, and `Walk` has no `Precedes` |
| enclosing fn by smallest containing span (`0_effects.sql:53-56`) | **none**: `#has-ancestor?` is boolean and binds no ancestor capture. `scripts/quality-gate.sh:50-72` runs the same lookup in Python by line. |
| effect rule `detail LIKE pattern` | **none**: needs the checker path, not syntax |
| cst mutation markers (4 kinds) | `[(mutable_specifier) (assignment_expression) (compound_assignment_expr) (unsafe_block)] @m` |
| interior callees (`1_mutation.sql:24-32`) | **none**: checker path |
| recursive propagation over `resolved_edge` | **none**: the engine runs per file |

Unit mismatch in our SQL: `unit_direct.markers` matches by byte span (`1_mutation.sql:39`), but `unit_direct.effects` matches by line range (`:40-46`). `effect_site.line` is computed with `readfile()` (`0_effects.sql:51-52`), although `line_start` (286 rows) is already in the db.

## 3. Minimal TypeSpec set

Directory: `crates/sprefa-extract/schema/sem/`. Compiled by `schema/2_gen.mjs` the same way as `bench/0_bench.tsp` (`emitSQL` plus a local pass, `2_gen.mjs:72-86`). This adds 2 core files and 1 data file per language. Project policy lives with the project.

```
schema/sem/0_sem.tsp        core: decorators, vocab enums, rule/policy tables (no language, no project names)
schema/sem/0_sem_lib.mjs    $decorators via decorator-def listDec; getters for 2_gen
schema/sem/1_rust.tsp       @@callee / @@kind augments for lang "rust"
crates/sprefa-extract/sql/0_sem.sql   generic views (authored SQL; TypeSpec has no expression vocabulary)
<project>/…/policy.tsp      @@owner / @@forbid, e.g. boop: @@owner(Effect.tmux, "boop-mux")
```

Sketch. Unverified on compiler 1.10.0: augment decorators on enum members and `Reflection.EnumMember` targets. Step 2 compiles a fixture first.

```tsp
// 0_sem.tsp
namespace Sem;
using TypeSpec.Reflection;
/** Sites whose checker callee path matches `pattern` (SQL LIKE) carry the member. */
extern dec callee(target: EnumMember, lang: valueof string, pattern: valueof string);
/** cst nodes of exactly `kind` (must exist in grammar_kind for lang) carry the member. */
extern dec kind(target: EnumMember, lang: valueof string, kind: valueof string);
/** Only `package` may hold sites of the member. */
extern dec owner(target: EnumMember, package: valueof string);
/** `package` may hold no site of the member, direct or reached over resolved_edge. */
extern dec forbid(target: EnumMember, package: valueof string);

enum Effect { process, signal, fs, sqlite, tmux, http, ws, net, jsonrpc, stdio, env, clock,
              thread, task, channel, lock, arena, log }       // the 18 from effect_rule
enum Mutation { mutable_binding, assign, compound_assign, unsafe_block, interior }
enum Plane { callee, kind }
enum Stance { owner, forbid }

model Rule {   // table rule, rows from @callee/@kind
  @Entity.pk id: int64;
  @Entity.unique(Rule.lang, Rule.plane, Rule.pattern, Rule.vocab, Rule.tag) lang: string;
  plane: Plane; pattern: string; vocab: string; /* "effect" | "mutation" */ tag: string;
}
model Policy { // table policy, rows from @owner/@forbid
  @Entity.pk id: int64;
  @Entity.unique(Policy.vocab, Policy.tag, Policy.package, Policy.stance) vocab: string;
  tag: string; package: string; stance: Stance;
}

// 1_rust.tsp
@@callee(Sem.Effect.process, "rust", "std::process::%");
@@callee(Sem.Effect.tmux, "rust", "tmux_interface::%");
@@kind(Sem.Mutation.mutable_binding, "rust", "mutable_specifier");
@@callee(Sem.Mutation.interior, "rust", "%::RefCell::borrow_mut");
// … all 58 effect_rule rows and the 4 kinds and 13 interior patterns from 1_mutation.sql
```

New ryii facts, declared in `schema/1_facts.tsp` and written from Rust:

```tsp
model FilePackage { ...ExportRow; record: "file_package"; path: string; lang: string;
                    package: string | null; manifest: string | null; target_kind: string | null; }
model GrammarKind { ...ExportRow; record: "grammar_kind"; lang: string; kind: string; named: boolean; }
```

Views in `sql/0_sem.sql`. The column lists are fixed, and a test compares them to `pragma table_info`.

```
test_span(path,start,end)                 cfg_scope cfg='test' ∪ file_package.target_kind='test'
owner_fn(path,start,owner)                innermost node family='call' kind in (function,method,lambda) containing start
tag_site(path,start,vocab,tag,detail)     unresolved⋈rule(plane=callee, LIKE) ∪ node(cst)⋈rule(plane=kind) ∪ — minus test_span
fn_tag(path,name,vocab,tag)               recursive over resolved_edge (from 0_effects.sql:72-80)
violation(path,line,owner,vocab,tag,package,stance)  tag_site⋈file_package⋈policy; line via line_start
```

Emitter pieces:

| piece | state |
|---|---|
| DDL from `@Entity` models | exists: `hafley-tsp/packages/sql/src/5_emit_sql.ts:11` `emitSQL`. Enums become unchecked TEXT (commit 15fc79d9). |
| decorator state helpers | exists: `packages/decorator-def/src/decorator-factory.ts:100` `listDec` |
| enum values to JSON | exists, local: `2_gen.mjs:78-86` (bench) |
| decorator state / values to `INSERT` rows (`generated/9_sem_rows.sql`) | **missing**. `packages/rusqlite` only writes at runtime (`3a_value_writer.ts:42`). |
| authored views | **missing**. `emitSQL` emits only intern `_text` views (`5_emit_sql.ts:111`). Proposal: keep `0_sem.sql` hand-authored and add a column-contract test. |
| `#emit!` relation to SQL table | **missing** (Rust side, `ryii query`) |
| fact doc text in `5_facts.json` (to replace `read/schema.rs` prose) | **missing**. `1a_fact_emit.mjs:74` drops docs. |

## 4. Pre-filter vs post-filter

Share of test code, from boop-effects.db:
- `unresolved` in a test cfg span: 9,068. In a `/tests/` path: 7,460. Combined: 16,528 of 33,313.
- `resolved_edge` callers in a test cfg span: 4,053. In `/tests/`: 2,752. Combined: 6,805 of 11,877.
- `site` in a test cfg span: 12,999 of 44,356.
- cst nodes in a test cfg span: 160,353 of 826,969.
- Test cfg bytes: 1,093,001 of 7,923,935 (overlap from 14 nested rows not removed).

All 6 counting queries over the full db ran in 1.66 s total.

| placement | mechanism | cost / reach | data loss |
|---|---|---|---|
| ryii flag `--cfg-skip test` | skip descendants of `cfg_scope` spans in `walk_file` (`rust_checker_ra.rs:371-382`, which visits every `CallExpr` / `MethodCallExpr`). Dropping `"test"` from the fast-tier cfg list (`7a_rust_checker_project.rs:88`) or `set_test` (`8_rust_checker_session.rs:204`) only deactivates the code: `walk_file` still visits it and the sites come back unresolved. | removes up to about 52% of checker-answered sites on this corpus; wall time unmeasured | facts gone; rename/move/cleave need test code, so the flag must be opt-in and analysis-only |
| SQL view (`test_span`) | `not exists` against `cfg_scope` plus package target kind | per query; reusable; language-neutral wherever a producer emits `cfg_scope` (rust only today) | none |
| scm++ predicate | needs a new `Walk::Precedes` (attribute sibling) or `#not-within? @c "cfg_scope" "test"` reading the per-file aux bundle; binary search per candidate | per match, per file | none; covers only `ryii query` / bundled `.scm` |

## 5. Ordered steps (one commit, one test each)

1. **`sql/0_sem.sql` generic views**, ported from `plans/boop-effects/*.sql`: `line_start` replaces `readfile`, and byte spans are used throughout. Test: `tests/NNN_sem_views.rs` builds a db from a fixture crate with a `#[cfg(test)] mod`, one `std::process` call, and one `let mut`, then runs `toMatchInlineSnapshot`-style insta on `tag_site`.
2. **`schema/sem/0_sem.tsp` + `0_sem_lib.mjs`**, with a fixture compile of the `@@` augment on enum members. Test: `2_gen.test.mjs` asserts that `generated/9_sem.sql` executes in sqlite3.
3. **Rows pass in `2_gen.mjs`**: decorator state becomes `generated/9_sem_rows.sql`. Test: a snapshot of the rows, and a `gen --check` stale check.
4. **`schema/sem/1_rust.tsp`** carries the 58 effect_rule rows and the mutation rules. Test: over boop-effects.db, `rule ⋈` reproduces `effect_site` = 2,293 and `mutation_site` = 3,011.
5. **`file_package` fact.** `RustTarget` gains `package` and `target_kind`, and the table is written under `--resolve`. Test: a workspace fixture snapshot. Unblocks `violation`.
6. **`grammar_kind` fact** (from `Language::node_kind_count`). Test: a `rule(plane=kind)` row whose kind is unknown makes the view check return the row.
7. **`violation` view + `gate/effect_owner.sql`.** Test: fixture with an owner package and an intruder. On boop: `tmux` sites are only in boop-mux (39), so 0 violations are expected. `sqlite` sites: boop-store 782, boop-harness 59, boop 22, boop-proc 1. `process` sites are spread over 6 crates.
8. **`ryii query --sqlite` writes `#emit!` relations as tables**, named by relation with columns from the emit keys. Test: an 8a scope query over a fixture, with a table snapshot.
9. (Opt-in) **`--cfg-skip test`** declared in `schema/cli/ops.tsp`, plus the `walk_file` skip. Test: fixture row counts with and without the flag, plus timing on boop.
10. (Optional) **scm++ `Walk::Precedes` or `#not-within?`.** Test: a predicate table row in `crates/hafley_scm/tests`.

## Open questions

1. Should the effect vocabulary be a closed enum in core, or open strings with the members defined by `@@callee` rows? Is `tmux` / `jsonrpc` / `arena` core or per-project, for example through `enum BoopEffect { ...Sem.Effect, tmux }`?
2. Does `mutable_specifier` (`let mut`, `&mut` params) count as impure? It drives most of the 628 units with markers>0.
3. Which test definitions apply: `cfg(test)` only, or also `#[test]` fns, `tests/` targets (cargo `target_kind=test`), dev-only crates?
4. What is the owner key: the Cargo package name, the crate dir, or the manifest path? Does the key extend to npm/go modules through the same `file_package.package`?
5. Where does `owner`/`forbid` policy live: in hafley-rs `schema/sem/`, or in the analyzed repo (boop)?
6. Should the decorator lib and the rows emitter live in `hafley-tsp/packages/sql`, shared, or as local `.mjs` next to `2_gen.mjs`?
7. Should `fn_tag` propagation use only checker edges (`resolution_origin='checker'`, 11,725 of 11,877), or all edges?
8. Is `--lines` mandatory for analysis runs, so that `line_start` is always present?
