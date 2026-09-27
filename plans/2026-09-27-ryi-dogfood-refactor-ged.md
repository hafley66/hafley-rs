# ryi dogfood: redundancy, type-subgraph GED, cleave trial (2026-09-27)

Scope: `crates/sprefa-extract/src` and `crates/hafley_scm/src` (186 files: 185 `.rs`, 1 `.mjs`).
Binary: `/Users/chrishafley/.cache/boop/cargo-target/release/ryii` (build 83a09e387a78, 2026-09-27T16:28Z).
Scratch (scripts, facts db, json): `~/.cache/lanes/claude-375/ged/`.

## 0. Fact extraction

```
ryii --resolve --kinds type,call --arms call,type --lines --root <worktree> \
     --sqlite ~/.cache/lanes/claude-375/ged/facts.db \
     crates/sprefa-extract/src crates/hafley_scm/src
```

Wall 2.7 s. Row counts: node 686,267; sig 10,870; resolved_edge 10,400; resolved_type_edge 5,799;
site 37,983; resolved_import 2,104; unresolved 28,717; method_owner 1,183; file 186.

Type-family nodes: function 1,983, method 1,146, struct 547, enum 140, alias 54, const 29, trait 17.

Item join: every `family=type` node (name span) is joined to its enclosing cst item
(`struct_item`, `enum_item`, `function_item`, ...) by span containment; struct fields and enum
variants are read from `field_declaration` / `enum_variant` cst nodes inside the item span.
Callers of a `resolved_edge` are the innermost `function_item` containing `caller_site_start`;
callees are matched by `(callee_path, callee_start)` to the item name span (524 edges unmatched).

### ryi behaviour observed while extracting

| # | observation | evidence |
|---|---|---|
| 1 | `--resolve` without `--arms` writes 0 `resolved_type_edge` rows | first run: 0 rows; with `--arms call,type`: 5,799 |
| 2 | `--lines` wrote 0 `line_start` rows; `span_lines.line` is 1 for every span | `select count(*) from line_start` = 0 |
| 3 | `--kinds type,call` still wrote cst and df families | node rows by family include cst and df (e.g. df `var_read` 43,757) |
| 4 | `--resolve` and `--deps` are mutually exclusive (clap error) | `error: the argument '--resolve' cannot be used with '--deps'` |
| 5 | `ryii fast` has no `--kinds`/`--resolve`/`--sqlite`-with-resolve; the resolve path is the top-level command | `ryii fast --help` |
| 6 | self `uses` type edges | `Deadline -> Deadline`, `Lines -> Lines` in `0_graph.rs` |
| 7 | `cleave` logs one INFO line per file of the whole git root (boop, ghcache, ...), 376.9 KB stderr per dry run | first dry run; `RUST_LOG=warn` silences it |

## 1. Redundancy and smells

Generated files excluded from type/duplicate analysis: `scip_proto.rs`, `server_auto.rs`,
`kotlin_scm_generated.rs`. (`cpg_proto.rs` is hand-transcribed and stays in.)

### 1a. Duplicated type shapes (structs/enums with >= 2 slots)

| measure | count |
|---|---|
| exact groups (same kind, same sorted `(field, type)` set) | 27 |
| types in exact groups | 59 |
| groups with same field-name set (types may differ) | 38 |
| types in those groups | 83 |

Exact groups with more than 2 members:

| slots | members |
|---|---|
| 2 `{start:u32,end:u32}` | `Span` sprefa-extract/src/5_diff.rs:18, `SpanJson` 5_diff.rs:302, `ByteRange` hafley_scm/src/read/lang/3_source_facts.rs:82, `SpanOut` hafley_scm/src/read/types.rs:2704 |
| 2 `{start:u32,len:u32}` | `Span` hafley_scm/src/lang/rust/11_df_syntax_rows.rs:12, 13_tsi_syntax_rows.rs:12, hafley_scm/src/span.rs:5 |
| 2 enum `{Named(String),Unknown}` | `TypeBinding` sprefa-extract/src/edit/rust_rename.rs:1198, hafley_scm/src/lang/rust/12_receiver_rows.rs:107, 22_tree_receiver_rows.rs:9 |
| 2 | `GoCheckerAnswer` go_checker.rs:39, `CheckerAnswer` rust_checker.rs:33, `TsCheckerAnswer` ts_checker.rs:33 |

Exact pairs (selection): `ResolvedImport` rust_modules.rs:749 = ts_resolve.rs:824 (8 slots);
`ImportRow` rust_modules.rs:763 = ts_resolve.rs:812 (6); `GoCheckerRef` go_checker.rs:23 = `TsCheckerRef` ts_checker.rs:18 (6);
`Arg` 13_tsi_syntax_rows.rs:50 = read/tsi/types.rs:33 (5); `DfArg/DfField/DfLit/DfLoop/DfParam` 11_df_syntax_rows.rs:92-115 = read/types.rs:959-998;
`WireFile/WireCosts/WireStats/WireLine/DriverRequest` go_checker.rs:482-526 = ts_checker.rs:302-346.

### 1b. Duplicated functions (non-test)

| measure | count |
|---|---|
| groups with identical resolved callee set (>= 3 callees) | 23 groups, 52 fns |
| groups with identical `sig` multiset (>= 3 slots, body >= 5 lines) | 180 groups, 651 fns |
| free-function names defined in > 1 file | 160 names |

Largest same-name free-function sets: `run` 11, `named_children` 10, `span` 9, `text` 8, `parse` 7,
`answer` 7, `collect` 6, `project_df` 6, `run_to` 5, `collect_items` 5, `def_span` 5, `df_push` 5.

`named_children` definitions: prolog_rename.rs:523, lang/rust/17_tree_entity_rows.rs:278,
18_tree_type_candidate_rows.rs:757, 20_tree_module_specifier_rows.rs:229, 21_tree_module_resolution_rows.rs:370,
22_tree_receiver_rows.rs:511, kotlin_type_edges.rs:780, prolog/_0_source.rs:51 (+2).

Largest sig groups:

| sig | fns | example sites |
|---|---|---|
| `(Node) -> Option<String>` | 27 | 11_df_syntax_rows.rs:2218, 22_tree_receiver_rows.rs:227, go.rs:650, python/_0_source.rs:621 |
| `(Node, TsiScope, _, Strings, TsiNames, TsiState)` | 16 | go_type_edges.rs:92..547, kotlin_type_edges.rs:113..585, python/_1_type_edges.rs:117..592 |
| `(Node) -> Span` | 16 | kotlin_rename.rs:311, go.rs:219, kotlin.rs:79, markdown/_0_source.rs:27 |
| `(Node) -> String` | 16 | 13_tsi_syntax_rows.rs:1234, go.rs:2646, prolog/_0_source.rs:84 |
| `(Node, _, Strings, CallF/FamilyBundle)` | 14 | go.rs:753, prolog/_0_source.rs:348, python/_0_source.rs:408 |
| `(MoveCx, ImportRef) -> Option<String>` | 8 | rust_rehome.rs:798/817/825, ts_rehome.rs:256/282/326/522, ts_rehome/cross.rs:170 |

Checker-tier body similarity (rapidfuzz normalized Levenshtein on item text):

| item | go_checker.rs line | ts_checker.rs line | lines | go~ts | go~rust_checker |
|---|---|---|---|---|---|
| stamp_digests | 400 | 224 | 21 | 1.000 | 1.000 |
| answer_of | 432 | 256 | 27 | 0.992 | 0.992 |
| call_at | 209 | 187 | 9 | 0.995 | 0.995 |
| into_fact | 550 | 370 | 18 | 0.990 | - |
| into_refs | 532 | 352 | 14 | 0.989 | - |
| fmt | 74 | 68 | 11 | 0.986 | 0.724 |
| answer | 463 | 287 | 7 | 0.964 | 0.842 |
| build | 122 | 115 | 78/69 | 0.878 | 0.578 |

go_checker.rs and ts_checker.rs share 19 item names out of 29 and 24.

### 1c. God files (z-scores over 186 files; fan = distinct files over call+type+import edges)

| file | lines | items | fan_in | fan_out | z_sum |
|---|---|---|---|---|---|
| hafley_scm/src/read/types.rs | 3645 | 187 | 78 | 17 | 19.42 |
| hafley_scm/src/read/project.rs | 3524 | 128 | 20 | 33 | 13.16 |
| hafley_scm/src/read/lang/ts.rs | 5293 | 179 | 9 | 14 | 12.86 |
| hafley_scm/src/read/lang/go.rs | 4884 | 151 | 5 | 10 | 10.39 |
| hafley_scm/src/read/lang/python/_0_source.rs | 3621 | 114 | 6 | 8 | 7.43 |
| hafley_scm/src/read/lang/rust_modules.rs | 2766 | 106 | 8 | 11 | 6.81 |
| sprefa-extract/src/edit/_7_cleave.rs | 2798 | 92 | 1 | 14 | 6.04 |
| hafley_scm/src/read/lang/mod.rs | 177 | 2 | 11 | 45 | 5.72 |
| sprefa-extract/src/bin/ryi.rs | 1184 | 40 | 0 | 33 | 5.13 |
| sprefa-extract/src/edit/rust_rename.rs | 2283 | 101 | 1 | 9 | 4.94 |

Files with any single z >= 2: 20.

### 1d. Dead items

Rule: non-test, not a trait-impl method, name not in {main,new,default,fmt,from,drop}, zero inbound
resolved call edges and zero inbound resolved type edges.

| measure | count |
|---|---|
| fact-dead (no inbound call/type edge) | 407 |
| text-confirmed (word-token count of the name across `crates/**/*.rs` <= its definition count) | 25 |
| text-confirmed, generated `scip_proto.rs` (`as_str_name`/`from_str_name`) | 20 |
| text-confirmed, hand-written | 5 |

Hand-written text-confirmed dead items:

| item | file:line | lines | pub |
|---|---|---|---|
| `load_by` | hafley_scm/src/read/lang/fact.rs:122 | 41 | yes |
| `jsonl_input` | sprefa-extract/src/bin/ryi/gen/server_auto.rs:162 (generated file) | 13 | no |
| `enter_callable` | hafley_scm/src/read/lang/ts_receivers.rs:314 | 12 | no |
| `exports_local` | hafley_scm/src/read/lang/ts_resolve.rs:1024 | 5 | yes |
| `names_a_module` | hafley_scm/src/read/lang/rust_modules.rs:1987 | 3 | yes |

The 382 fact-dead items without text confirmation include names shared with other definitions
(`new`-like collisions), fn-pointer uses, and macro-body uses; they are unverified.

### 1e. Cyclic file dependencies

File graph = cross-file resolved call + type + import edges, with module-root files
(`mod.rs`, `lib.rs`, `edit.rs`, `main.rs`) removed so `mod` declarations do not close cycles.

| measure | count |
|---|---|
| SCCs with > 1 file | 6 |
| SCC sizes | 46, 3, 3, 2, 2, 2 |
| mutual 2-cycles | 33 |
| 2-cycles with `read/types.rs` as one end | 11 |

SCC(46) spans `hafley_scm/src/read/**` and `lang/rust/23_frontend.rs`. `read/types.rs` 2-cycles:
7_scm_rows.rs, go_checker.rs, go_modules.rs, kotlin_modules.rs, rust_checker.rs, rust_modules.rs,
ts_checker.rs, ts_resolve.rs, scip.rs, trace.rs, tsi/sink.rs.
Other SCCs: {edit/_0_seams.rs, _1_move_cx.rs, _1_rename_cx.rs}; {walk/dispatch_by_direction.rs,
ts_ancestor_holds.rs, ts_descendant_holds.rs}; {types/1_emitted_fact.rs, types/match_arena.rs};
{edit/rust_rehome.rs, rust_rehome/cross.rs}; {edit/ts_rehome.rs, ts_rehome/cross.rs}.
Cross-crate edges: sprefa-extract -> hafley_scm 143, reverse 0.

## 2. Type subgraph GED

### Graphs

- Corpus type graph: node = struct/enum item (655 after exclusions); undirected edge when a
  `resolved_type_edge` has a type item as owner and a type item as target (663 edges: field 645,
  generic 18, uses 1). Weakly connected components: 188 (largest 306, then 21, 14, 12, 10, 10; 137 singletons).
- Per-type subgraph (radius 1): center node `role=type, label=<type name>`; one leaf per field or
  variant, `role=slot, label=<field type text | variant payload text | "">`; edge center->leaf
  `kind in {field, variant}, name=<field or variant name>`.
- Pairs compared: both types have 2..24 slots (569 types) and lie in DIFFERENT components;
  size-difference bound `1 - 2|na-nb|/norm < 0.55` skipped. 106,581 pairs scored.

### Cost model

- node subst = `rapidfuzz.distance.Levenshtein.normalized_distance(label1, label2)` if roles match, else 1; node ins/del = 1.
- edge subst = `0.25*[kind1 != kind2] + 0.75*Levenshtein.normalized_distance(name1, name2)`; edge ins/del = 1.
- grade = `1 - GED / (|V1|+|E1|+|V2|+|E2|)` (denominator = delete G1 + insert G2).

Computation: all pairs by exact star-GED assignment (`scipy.optimize.linear_sum_assignment`, center
fixed to center, unmatched leaf = 2); the top 60 re-solved with `networkx.optimize_edit_paths` under
the same callbacks (upper bound = assignment cost, timeout 5 s); all 60 returned from networkx.
Grade histogram over 106,581 pairs: [0,.5) 51,794; [.5,.6) 36,934; [.6,.7) 15,964; [.7,.8) 1,708;
[.8,.9) 137; [.9,.95) 14; [.95,1] 30.

### Top 25 disconnected pairs

Center node always aligns to center; alignment lists `field: type -> field: type`.

| # | type A | file A | type B | file B | GED | norm | grade | alignment |
|---|---|---|---|---|---|---|---|---|
| 1 | WireLine | hafley_scm/src/read/lang/go_checker.rs:526 | WireLine | hafley_scm/src/read/lang/ts_checker.rs:346 | 0.0 | 10 | 1.0000 | File(WireFile)->File(WireFile); Stats(WireStats)->Stats(WireStats) |
| 2 | WireCosts | go_checker.rs:515 | WireCosts | ts_checker.rs:335 | 0.0 | 14 | 1.0000 | load_ms:u64->load_ms:u64; walk_ms:u64->walk_ms:u64; files:usize->files:usize |
| 3 | WireStats | go_checker.rs:507 | WireStats | ts_checker.rs:327 | 0.0 | 10 | 1.0000 | stats:WireCosts->stats:WireCosts; coverage:Vec<(String,bool,Option<String>)>->same |
| 4 | WireFile | go_checker.rs:496 | WireFile | ts_checker.rs:316 | 0.0 | 18 | 1.0000 | path:String->path; calls:Vec<WireRow>->calls; types:Vec<WireRow>->types; tsi:Vec<Vec<Value>>->tsi |
| 5 | DriverRequest | go_checker.rs:482 | DriverRequest | ts_checker.rs:302 | 0.0 | 14 | 1.0000 | root:&'a Path->root; files:&'a [(String,PathBuf)]->files; tsi:bool->tsi |
| 6 | TsiState | hafley_scm/src/lang/rust/13_tsi_syntax_rows.rs:145 | TsiState | hafley_scm/src/read/lang/ts.rs:1292 | 0.0 | 10 | 1.0000 | called:BTreeSet<u32>->called; classes:BTreeMap<&'static str,u32>->classes |
| 7 | Arg | lang/rust/13_tsi_syntax_rows.rs:50 | Arg | hafley_scm/src/read/tsi/types.rs:33 | 0.0 | 22 | 1.0000 | Id(u32); Span(String,u32,u32); Text(String); Int(i64); Atom(String) all identity |
| 8 | Span | lang/rust/13_tsi_syntax_rows.rs:12 | Span | hafley_scm/src/span.rs:5 | 0.0 | 10 | 1.0000 | start:u32->start; len:u32->len |
| 9 | ReceiverBinding | lang/rust/12_receiver_rows.rs:27 | ReceiverBinding | read/types.rs:545 | 0.0 | 10 | 1.0000 | call_site:Span->call_site; outcome:ReceiverOutcome->outcome |
| 10 | DfLoop | lang/rust/11_df_syntax_rows.rs:115 | DfLoop | read/types.rs:998 | 0.0 | 14 | 1.0000 | span:Span; var:Option<String>; collection:Option<String> identity |
| 11 | DfLit | lang/rust/11_df_syntax_rows.rs:109 | DfLit | read/types.rs:989 | 0.0 | 14 | 1.0000 | node:NodeRef; kind:&'static str; text:String identity |
| 12 | DfField | lang/rust/11_df_syntax_rows.rs:103 | DfField | read/types.rs:978 | 0.0 | 14 | 1.0000 | owner:NodeRef; name:String; value:NodeRef identity |
| 13 | DfArg | lang/rust/11_df_syntax_rows.rs:97 | DfArg | read/types.rs:968 | 0.0 | 14 | 1.0000 | call:NodeRef; pos:i64; arg:NodeRef identity |
| 14 | DfParam | lang/rust/11_df_syntax_rows.rs:92 | DfParam | read/types.rs:959 | 0.0 | 10 | 1.0000 | node:NodeRef; pos:u32 identity |
| 15 | Span | lang/rust/11_df_syntax_rows.rs:12 | Span | hafley_scm/src/span.rs:5 | 0.0 | 10 | 1.0000 | start:u32; len:u32 identity |
| 16 | Span | lang/rust/11_df_syntax_rows.rs:12 | Span | lang/rust/13_tsi_syntax_rows.rs:12 | 0.0 | 10 | 1.0000 | start:u32; len:u32 identity |
| 17 | TypeBinding | sprefa-extract/src/edit/rust_rename.rs:1198 | TypeBinding | lang/rust/22_tree_receiver_rows.rs:9 | 0.0 | 10 | 1.0000 | Named(String); Unknown identity |
| 18 | TypeBinding | sprefa-extract/src/edit/rust_rename.rs:1198 | TypeBinding | lang/rust/12_receiver_rows.rs:107 | 0.0 | 10 | 1.0000 | Named(String); Unknown identity |
| 19 | GoCheckerError | go_checker.rs:64 | TsCheckerError | ts_checker.rs:58 | 0.1429 | 18 | 0.9921 | NotBuilt; NoDriver(String); Failed(String); Budget(u64) identity; center name cost 0.1429 |
| 20 | UseBindingRow | lang/rust/10_module_resolution_rows.rs:11 | UseBinding | read/lang/rust_modules.rs:28 | 0.2308 | 18 | 0.9872 | local:String; qualifier:Vec<String>; asked:String; reexport:bool identity; center 0.2308 |
| 21 | Property | read/cpg/cpg_proto.rs:15 | Property | read/cpg/cpg_proto.rs:36 | 0.1373 | 10 | 0.9863 | name:i32->name:i32; value:Option<super::PropertyValue>->value:Option<super::super::PropertyValue> (0.1373) |
| 22 | StarImportRow | lang/rust/10_module_resolution_rows.rs:19 | StarImport | read/lang/rust_modules.rs:38 | 0.2308 | 10 | 0.9769 | qualifier:Vec<String>; reexport:bool identity; center 0.2308 |
| 23 | RegionError | sprefa-extract/src/3_region_writer.rs:10 | RenameError | sprefa-extract/src/edit/_6_rename.rs:35 | 0.3636 | 10 | 0.9636 | message:String; exit:i32 identity; center 0.3636 |
| 24 | TsiFactRow | lang/rust/13_tsi_syntax_rows.rs:58 | FactOut | read/tsi/types.rs:45 | 0.6000 | 14 | 0.9571 | fact:u32; relation:String; args:Vec<Arg> identity; center 0.6 |
| 25 | Span | sprefa-extract/src/5_diff.rs:18 | SpanOut | read/types.rs:2704 | 0.4286 | 10 | 0.9571 | start:u32; end:u32 identity; center 0.4286 |

Pairs 26-30 in `ged.json`: SignatureSlot (7_type_entity_rows.rs:17) ~ SigSlot (read/types.rs:189) 0.9538;
SpanJson ~ SpanOut 0.95; 5_diff Span ~ {span.rs Span, 13_tsi Span, 11_df Span} 0.95 each;
DfNodeKind 11_df_syntax_rows.rs:23 ~ DfNodeKind read/types.rs:1082 0.9365 (GED 4.825, norm 76).

## 3. Ranked refactor targets

| rank | target | action | evidence |
|---|---|---|---|
| 1 | checker tier: go_checker.rs, ts_checker.rs, rust_checker.rs | extract shared wire types + shared fns into `read/lang/checker_wire.rs` / `checker_common.rs` | 5 GED 1.0 pairs (Wire*, DriverRequest), Error 0.9921; exact dup groups Answer x3, Ref x2; 19 shared names go/ts; bodies 0.96-1.00 (stamp_digests 1.000 x3, answer_of 0.992 x3, call_at 0.995 x3) |
| 2 | `lang/rust/11_df_syntax_rows.rs` vs `read/types.rs` df row types | one definition of DfParam/DfArg/DfField/DfLit/DfLoop/DfNodeKind/Span | 6 GED 1.0 pairs + DfNodeKind 0.9365; 13 shared item names |
| 3 | Span shapes | one `{start,len}` Span (span.rs) and one `{start,end}` span row | exact groups of 3 (`{start,len}`) and 4 (`{start,end}`); GED 1.0 x3, 0.95-0.957 x5 |
| 4 | tree-sitter node helpers across lang files | one shared helper module per crate for `named_children`/`span`/`text`/`node_text` | `named_children` 10 defs, `span` 9, `text` 8, `parse` 7, `def_span` 5; sig groups `(Node)->Option<String>` 27, `(Node)->Span` 16, `(Node)->String` 16 |
| 5 | `read/types.rs` | split by family (df / call / type / receiver / span rows) | 3645 lines, 187 items, fan_in 78 (z 8.32), z_sum 19.42; 11 of 33 file 2-cycles; member of SCC(46) |
| 6 | `lang/rust/10_module_resolution_rows.rs` vs `read/lang/rust_modules.rs` | one UseBinding / StarImport / ImplEntry | GED 0.9872, 0.9769; same field-name set ImplMethodsRow~ImplEntry |
| 7 | `rust_modules.rs` vs `ts_resolve.rs` import tables | shared `ResolvedImport`, `ImportRow`, `ResolvedImportKind`, `ExportTable` | exact dup ResolvedImport (8 slots), ImportRow (6); 12 shared item names; same component, so absent from GED list |
| 8 | `TypeBinding` x3 and `lang/rust/13_tsi_syntax_rows.rs` vs `read/tsi/types.rs` (Arg, TsiFactRow~FactOut, TsiState~ts.rs) | single definition | 3-way exact enum dup; GED 1.0 x4, 0.9571 |
| 9 | per-language df/tsi walkers (go.rs, kotlin.rs, python/_0_source.rs, *_type_edges.rs) | shared walker skeleton | `project_df` 6 defs, `df_push` 5; sig group of 16 `tsi_*` fns and 9 `*_flow_fn/_walk_fns/project_df` fns |
| 10 | SCC(46) in `hafley_scm/src/read/**` | break lang-file -> types.rs -> lang-file back-edges | 46-file SCC; 33 mutual 2-cycles (go.rs<->go_modules.rs, kotlin.rs<->kotlin_*.rs x3, ts.rs<->ts_resolve.rs) |
| 11 | dead code | delete after owner check | 5 hand-written text-confirmed items (fact.rs:122 load_by 41 lines, ts_receivers.rs:314, ts_resolve.rs:1024, rust_modules.rs:1987, server_auto.rs:162) |
| 12 | `RegionError` / `RenameError` | shared `{message, exit}` error | GED 0.9636 |

## 4. Cleave trial

Worktree `/Users/chrishafley/projects/hafley-rs/.claude/worktrees/agent-a180e426da91e86cb`,
branch `worktree-agent-a180e426da91e86cb`. All three applied with `--commit` (soopy stage;
cleave does not create git commits). Check: one `cargo check -j 2 -p hafley_scm --message-format short`,
default features (`read`: rust, go, typescript, ... ; no `go-checker`), `CARGO_TARGET_DIR=~/.cache/lanes/claude-375/target`. Exit 101, 3 errors.

| cand | command | dry run | commit | cargo check |
|---|---|---|---|---|
| A | `ryii cleave crates/hafley_scm/src/read/lang/go_checker.rs#WireLine crates/hafley_scm/src/read/lang/checker_wire.rs --drag` | plan ok (drag: WireFile moved, WireStats exported, WireRow exported; 2 passes) | applied | 2 errors (E0432 x2) |
| B | `ryii cleave crates/hafley_scm/src/read/lang/go_checker.rs#stamp_digests crates/hafley_scm/src/read/lang/checker_common.rs --drag` | plan ok (travel HashMap, ContentId) | applied | 1 error (E0603) |
| C | `ryii cleave crates/hafley_scm/src/lang/rust/17_tree_entity_rows.rs#named_children crates/hafley_scm/src/lang/rust/16a_tree_nodes.rs --drag` | plan ok | applied | no error attributed to C's files (resolve-phase errors from A/B may mask later-phase errors) |

Compiler output:

```
crates/hafley_scm/src/read/lang/go_checker.rs:20:5: error[E0432]: unresolved import `super::checker_wire::WireLine`
crates/hafley_scm/src/read/lang/checker_wire.rs:1:25: error[E0432]: unresolved imports `super::go_checker::WireRow`, `super::go_checker::WireStats`: no `WireRow` in `read::lang::go_checker`, no `WireStats` in `read::lang::go_checker`
crates/hafley_scm/src/read/lang/checker_common.rs:2:12: error[E0603]: module `_0_types` is private: private module
error: could not compile `hafley_scm` (lib) due to 3 previous errors
```

### cleave bugs

| # | cand | bug | detail |
|---|---|---|---|
| 1 | A | cfg gate dropped on inserted `use` lines | moved items carry `#[cfg(feature = "go-checker")]`; the inserted `use super::checker_wire::WireLine;` (go_checker.rs:20) and `use super::go_checker::{WireRow, WireStats};` (checker_wire.rs:1) are unconditional -> E0432 with the feature off |
| 2 | A, B | cfg gate dropped on new `mod` declaration | source module is declared `#[cfg(feature = "go")] pub mod go_checker;`; cleave appends `pub mod checker_wire;` / `pub mod checker_common;` with no cfg (read/lang/mod.rs:86-87) |
| 3 | A | moved target keeps private visibility while source imports it | dest has `enum WireLine` (no `pub`) but go_checker.rs imports `super::checker_wire::WireLine` -> E0603 expected with `go-checker` on (by inspection; not compiled) |
| 4 | A | moved struct fields stay private | `pub struct WireFile { path, calls, types, tsi }` private fields; go_checker.rs:642-650 reads `file.path`, `file.calls`, `file.types`, `file.tsi` -> E0616 expected with `go-checker` on (by inspection) |
| 5 | A | dragged dependency points back at source | dest imports `WireRow`, `WireStats` from `super::go_checker` (exported in place, `pub type WireRow`, `pub struct WireStats`) -> new file <-> source file 2-cycle; WireCosts left in source |
| 6 | B | re-export chain respelled through a private module | travel `ContentId from crate::read::types::ContentId as soopy::_0_types` -> `use soopy::_0_types::ContentId;` ; `read/types.rs:35` is `pub use soopy::ContentId;` and `soopy/src/lib.rs:1` is `mod _0_types;` (private) -> E0603. Valid spellings: `crate::read::types::ContentId` or `soopy::ContentId` |
| 7 | A, B vs C | inconsistent mod declaration shape | C writes `#[path = "16a_tree_nodes.rs"] pub(crate) mod tree_nodes;` on one line (mod.rs:51); A/B write `pub mod X;` without `#[path]` and append after a `#[cfg(feature = "typescript")]` item |
| 8 | C | `--text-refs` false positives | reports 4 lines in `crates/sprefa-extract/plans/2026-09-21-remove-astgrep-native-scm.md` (184, 191, 381, 384) as leftover SRC spellings to rewrite to the dest, while SRC file still exists with its other items |
| 9 | all | whole-repo extraction per cleave | cleave extracts every file under the git root (boop, ghcache, ...); INFO per file on stderr (376.9 KB for one dry run) |

Duplicates of the moved items in sibling files are left in place (ts_checker.rs `WireLine`/`stamp_digests`,
rust_checker.rs `stamp_digests`, 9 other `named_children`): cleave moves one item and has no merge-duplicates mode.

## Artifacts

`~/.cache/lanes/claude-375/ged/`: `extract.sh`, `facts.db`, `items.py` / `items.pkl`, `smells.py` / `smells.json`,
`dups.py` / `dups.json`, `ged.py` / `ged.json` (top 60 with alignments), `cleave.patch`, `cargo_check.txt`.
