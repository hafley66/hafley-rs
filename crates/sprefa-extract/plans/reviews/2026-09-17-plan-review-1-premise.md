# Adversarial review 1 of 3: premises

Worktree: `/Users/chrishafley/projects/hafley-rs/.boop-worktrees/review/plan-premise` at `a2046f2e`.
`git diff --stat 16ebd451 HEAD -- crates/sprefa-extract/src crates/sprefa-extract/docs crates/sprefa-extract/plans` = `2 files changed, 317 insertions(+)` (the two documents only), so every `file:line` receipt in them is checkable against this worktree.
`CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-review1`. Binary `crates/sprefa-extract/target-review1/debug/extract`.
`REPORT.md` was tracked at HEAD as a boop document (`0785495b`), read-only review law required writing this file at the worktree root; the original is recoverable from git and no commit was made.

## Falsified

| claim | doc location | what the code says | receipt |
| --- | --- | --- | --- |
| `FlatFact`, 57 variants | docs sec 2, row `fact` | 60 top-level variants | `src/types.rs:3006` `pub enum FlatFact`, closes at `:3691`; parsed variant headers: 3008,3011,3012,3013,3014,3015,3016,3026,3041,3051,3063,3074,3085,3096,3109,3116,3128,3137,3148,3158,3170,3182,3195,3213,3230,3242,3252,3262,3272,3285,3302,3312,3323,3343,3376,3385,3392,3401,3409,3416,3424,3432,3443,3462,3471,3483,3519,3530,3543,3556,3564,3573,3585,3597,3607,3635,3648,3660,3671,3685 = 60 |
| "Resolve legs, first answer wins: `same_file`, `module_plane`, `receiver`, `self_type`, `corpus_unique`, `checker`, `scip`" | docs sec 4b mermaid | rust is `receiver`, `module_plane` (qualified-path arm, exclusive), `self_type` (assoc), `self_type` (`Self::`), `same_file`, `module_plane` (import-bound), `corpus_unique`, then the scip fold, then the checker override. `same_file` is 5th, not 1st; `corpus_unique` is last of the name legs, not 5th | `src/lang/rust.rs:1151-1154` receiver, `:1160-1190` module arm, `:1191` assoc, `:1192` self, `:1193-1204` same_file, `:1205-1214` module_plane, `:1215-1225` corpus_unique, `:1237-1259` scip fold, `:1260-1295` checker |
| "`corpus_unique` and `same_file` run ahead of a spelled receiver" | docs sec 7, guess-legs row | rust: receiver leg is first (`recv_t` gates everything at `:1151`). go: `Some(ReceiverOutcome::Named)` is the first arm (`:4227`); a `pkg.Func` shadowing receiver is the first arm of the no-receiver branch (`:4270-4290`). ts: the module-plane arms run first (`:4934-4942`); the receiver leg precedes the name match for every receiver except a `TypeBinding::Field` (destructured) one (`:4945-4955`) | `rust.rs:1151`, `go.rs:4227`, `go.rs:4270-4290`, `ts.rs:4934-4955` |
| "the 148 false `push` edges are `corpus_unique` picking `RustCallDefs::push`" | docs sec 7 | The 155 rows split 118 `corpus_unique` + 30 `same_file` + 7 `receiver`; 148 = 118 + 30, so the doc's number is the two guess legs together, not `corpus_unique` alone | CTF reproduction below |
| "148 false `push` edges" / "`push` shows 148 false before lane 2" | docs sec 1, sec 3d; plan sec 2 table | The stated query returns 155 rows: `src/bin/extract.rs` 2, `src/cfg.rs` 5, `src/lang/astgrep.rs` 2, `src/lang/extract_lang.rs` 1, `src/lang/go.rs` 22, `src/lang/kotlin.rs` 17, `src/lang/rust.rs` 37, `src/lang/ts.rs` 27, `src/types.rs` 3, `src/wire.rs` 39 | CTF reproduction below |
| "3 of 4 `project` callers missed"; plan pass condition "`CstProjector.project` callers 4 of 4" | docs sec 1; plan sec 2 lane 2 | 8 call sites named `project` in the corpus (`site` rows). Five are `CstProjector.project`: `src/lang/astgrep.rs:265`, `src/lang/go.rs:2757`, `src/lang/kotlin.rs:1626`, `src/lang/rust.rs:3367`, `src/lang/ts.rs:4073`. Three (go, kotlin, rust) have no `resolved_edge`; two have one. The denominator is 5, not 4 | `sqlite3 ctf18.db "SELECT _input_path, span__start FROM site WHERE callee='project'"` = astgrep 10642, go 112220, kotlin 68544, rust 131448, ts 158253/159353/160475/161164; CTF query 2 below |
| "spelled receiver `CstProjector.project` unresolved" | docs sec 3d | `src/lang/ts.rs:4073` IS resolved: `callee_path=src/lang/ts.rs`, `resolution_origin=same_file`. `CstProjector::project` is declared at `src/lang/astgrep.rs:182` (the only `fn project` in that file); ts.rs declares only `TypeProjector::project` (:182), `CallProjector::project` (:1898), `DfProjector::project` (:2852). The edge exists and names the wrong file | CTF query 2 below; `read src/lang/astgrep.rs:179-182`, `read src/lang/ts.rs:179,1895,2849` |
| "`same_file` beats the spelled receiver in ts.rs" (cited to `ts.rs:4073`) | docs sec 3d, sec 4b | The site row for that call is `('src/lang/ts.rs', 158253, 158260, 'project', callee_path=NULL)`: no receiver spelling is recorded for `Expr.method(...)` at all, so no receiver leg was eligible to lose. The file `src/lang/ts.rs` is a `.rs` file, owned by `RustSource` (`sources()` first match), so no ts-plane leg ordering is involved | `site` row above; `src/lang/mod.rs:114-127` roster order |
| "rust answers no `corpus_unique`" | docs sec 3d (row quoting `tests/90_mutation_battery.rs:24-27`) | rust stamps `CorpusUnique` in the type plane and the call plane; the CTF emits 813 `corpus_unique` rows corpus-wide, 187 with `caller_path='src/lang/rust.rs'` | `src/lang/rust.rs:459`, `:493`, `:1222`; CTF origin histogram below; the quoted header is stale at `tests/90_mutation_battery.rs:26-29` |
| `method_owner{owner, self_type, trait}` listed as a Type plane row, "(rust only fills)" | docs sec 5c | `method_owner` is a CallF aux row (`CallFAux.method_owners`, family=call), not a TypeF row: `TypeFAux` holds only `sigs, consts, candidates, docs, doc_nodes, impl_owners, tsi`. go fills it too (`self_type` Some, `trait_name` None) | `src/types.rs:845-846`, `src/types.rs:3228-3240`; `src/schema.rs:34` `record=method_owner family=call`; `src/lang/go.rs:883-887`, `:945-948` |
| lane 1.1 receipt "`src/types.rs:3400-3480` row structs" for `scip_def`/`scip_ref`/`scip_fn_edge` | plan lane 1.1 | `ScipDefRow` 3376-3383, `ScipNameRow` 3385-3390, `ScipRefRow` 3392-3399, `ScipEdgeRow` 3401-3407, `ScipFnEdgeRow` 3409-3414. 3400-3480 covers `scip_ref` through `size_skip`, not `scip_def` | `src/types.rs:3376-3414` |
| sec 6 CodeQL: "JS/Python/Ruby via own parsers" | docs sec 6, `how facts get made` | JS/TS extraction in CodeQL is driven by the TypeScript compiler (the extractor ships/uses the TS compiler API), not an own parser. Python and Ruby use their own parsers | memory, no local receipt; confidence high for the JS/TS half, medium for Ruby |
| sec 6 CodeQL: "incremental: no; a database is one build" | docs sec 6 | GitHub code scanning runs overlay/incremental database builds (base + overlay) for several languages, so "incremental: no" is too strong; a single `codeql database create` is one build | memory; confidence medium |
| sec 6 CodeQL: "crawl many repos ... without building: no (compiled langs need the build)" | docs sec 6 | CodeQL supports `build-mode: none` for Java/Kotlin, C#, C/C++ and Swift in default setup, which extracts without invoking the build | memory; confidence medium |
| sec 6 Joern: "cross-file name binding: fuzzy by design; a type recovery pass guesses" | docs sec 6 | `javasrc2cpg` runs a JavaParser-based symbol solver and `jssrc2cpg` uses the TypeScript compiler, so their name binding is type-assisted, not fuzzy. The "fuzzy by design" description fits `c2cpg` | memory; confidence medium |
| sec 6 Joern: "gosrc2cpg (own parser)" | docs sec 6 | `gosrc2cpg` shells out to an astgen Go program that parses with the Go standard library (`go/parser`, `go/ast`); "own parser" overstates it | memory; confidence medium |

## Confirmed

| claim | doc location | what the code says | receipt |
| --- | --- | --- | --- |
| receipts are valid at `16ebd451` and at HEAD | doc header | HEAD differs from `16ebd451` only by adding the two documents | `git merge-base --is-ancestor 16ebd451 HEAD` = true; `git diff --stat 16ebd451 HEAD` = 2 files, 317 insertions |
| sec 7 "resolve legs split into deterministic and guess legs" | docs sec 7 | legs are stamped per edge via `resolution_origin`; the enum holds both families | `src/types.rs:1660` `ResolutionOrigin`; CTF origin histogram is exactly `same_file, corpus_unique, receiver, self_type, module_plane` |
| rust `Resolve<CallF>` decision sites `:454-493` and `:1154-1222` | docs sec 2, sec 7 | `:426-461` is `resolve_type_dst` (type plane), `:465-495` `name_match_type_dst`, `:1010-1309` the call arm with the name legs at `:1150-1225` | `src/lang/rust.rs` |
| rust unresolved-reason chain `:1414-1424` | docs sec 2 | `checker_external -> External`, `inferred -> Inferred`, `external_prefix/PRELUDE -> External`, `corpus_defs empty -> NoCorpusDef`, else `Ambiguous` | `src/lang/rust.rs:1414-1426` |
| `KtModuleIndex` (built `project.rs:429`) and `PyModuleIndex` (built `project.rs:420`) are never read by kotlin/python resolve | docs sec 5a | zero references in `src/lang/kotlin.rs` or `src/lang/python/*.rs`; kotlin's call arm is `KotlinSource::call_name_match` -> `CorpusUnique` only; the only reads of the fields are the resolved_import row closures | grep `KtModuleIndex\|PyModuleIndex` -> project.rs:29-31,420-432, `types.rs:1957-1959`, `kotlin_modules.rs:104-124`, `python/_2_modules.rs:249-278`, `lang/mod.rs:7`; `project.rs:1995` (py) and `:2009` (kt) read them for `ResolvedImportRow`; `kotlin.rs:1728-1763` |
| `resolved_import` emitters: ts 1948, rust 1962, go 1981, python 1995, kotlin 2009 | docs sec 5a | exact | `src/project.rs:1948,1962,1981,1995,2009` |
| stale comment at `types.rs:3665-3669`: "go and rust arms ... neither emits it today" | docs sec 5a | the comment is at 3664-3667 verbatim; all five emit the row | `src/types.rs:3664-3667`; see emitters above |
| `scip_occurrence` carries `start`/`end` + `enclosing_start`/`enclosing_end` | docs sec 4c (`types.rs:3483-3514`) | exactly those fields, plus roles bitfield, seven role flags, syntax_kind, optional `text` | `src/types.rs:3483-3518` |
| `scip_def`/`scip_ref`/`scip_fn_edge` are symbol-only | docs sec 4c, plan lane 1.1 | `ScipDefRow{symbol,file,repo}`, `ScipRefRow{file,symbol,def_file,repo}`, `ScipFnEdgeRow{caller,callee}`; no span fields | `src/types.rs:3376-3383,3392-3399,3409-3414` |
| `flatten_scip_records` at `scip_rows.rs:116` | docs sec 4c | exact line; `SCIP_RECORD_KINDS` has 10 kinds and does not include `scip_def`/`scip_ref`/`scip_fn_edge` | `src/scip_rows.rs:29-40,116` |
| `ArgToParam`/`RetToCallRes` are emitted by `flow_edges` under resolve; `LambdaElem`/`LambdaRet` are never constructed | docs sec 5b | `FlowEdgeKind::ArgToParam` at `types.rs:1334`, `RetToCallRes` at `:1353`; `flow_edges` called at `project.rs:581` under `request.arms.flow`; `LambdaElem`/`LambdaRet` appear only at `types.rs:1253-1255` (decl) and `:1263-1264` (`as_str`) | `src/types.rs:1248-1264,1289-1359`; `src/project.rs:580-582` |
| crate `AGENTS.md:52-54` is stale on those two | docs sec 5b | lines 51-54 say arg->param/ret->call-res "is DERIVED in the engine", while the crate emits both | `crates/sprefa-extract/AGENTS.md:50-55` |
| sec 3c move/rename test counts 20+8+4, 18, 4+4+5, 5, 18, 8, 3, 4 | docs sec 3c | all twelve counts match | counted `#[test]` per file: `1_move.rs` 20, `42_move_list.rs` 8, `38_move_perf.rs` 4, `3_move_rust.rs` 18, `41_move_ts.rs` 4, `2_move_refs.rs` 4, `5_move_scip.rs` 5, `4_move_kotlin.rs` 5, `4_rename_ts.rs` 18, `5_rename_rust.rs` 8, `7_rename_kotlin.rs` 3, `8_rename_prolog.rs` 4 |
| sec 3c go/python have 0 move/rename tests and are absent from `rehomes()`/`renames()` | docs sec 3c | no `*move*go*`/`*rename*go*`/`*move*python*`/`*rename*python*` test file exists; `rehomes()` = rust, kotlin, prolog, ts; `renames()` = ts, rust, kotlin, prolog | `tests/*.rs` inventory (170 files); `src/lang/mod.rs:133-171,173-186` |
| roster membership `src/lang/mod.rs:116-186` | docs sec 3a/3c | `sources()` 11 entries at `:114-127`, `rehomes()` 4 at `:133-171`, `renames()` 4 at `:173-186` | `src/lang/mod.rs:110-190` |
| sec 3b `src/scip.rs` receipts 119-132, 133-146, 156-169, 180-190, 194-207, 208-221 | docs sec 3b | all six `impl ScipSource for ...` blocks start exactly at those lines | `src/scip.rs:119,133,156,180,194,208` |
| sec 3b "`scip_ensure.rs:62-105` has 6 rows" | docs sec 3b | `INDEXERS` = rust, typescript, python, go, kotlin/java, cpp | `src/scip_ensure.rs:62-105` |
| sec 3b npx pin 0.4.0 and `go run` pin v0.2.7 | docs sec 3b | `Fallback::Npx("@sourcegraph/scip-typescript@0.4.0")`; `Fallback::GoRun("...scip-go@v0.2.7")` | `src/scip.rs` `TS_SPEC`, `GO_SPEC` |
| sec 3b checker receipts `rust_checker_ra.rs:32-89`, `ts_checker.rs:406`, `ts_checker.mjs:15-34`, `go_checker.rs:583-630`, `project.rs:773-798` | docs sec 3b | `pub fn answer` in the 32-89 window; `dir.join("ts_checker.mjs")` at 406; `loadTypeScript(root)` 15-34; the `~/.cache/sprefa/go_checker` build 583-630; semantic-run emission for ts/go/rust 773-798 | those lines |
| sec 3a parser pairs: rust ast-grep/syn, ts ast-grep/oxc, go ast-grep/tree-sitter-go, kotlin ast-grep/tree-sitter-kotlin-sg, python ast-grep/tree-sitter-python, prolog/markdown/gdscript/commonlisp tree-sitter | docs sec 3a | `syn::parse_file` in rust.rs and rust_modules.rs; `oxc_resolver` in ts_resolve.rs; `tree_sitter_go`, `tree_sitter_kotlin_sg::LANGUAGE`, `tree_sitter_md::LANGUAGE` | `src/lang/rust.rs:3`, `src/lang/ts_resolve.rs:23`, `src/lang/go.rs:1-11`, `src/lang/kotlin.rs:60`, `src/lang/python/_0_source.rs:1-3`, `src/lang/markdown/_0_source.rs:125` |
| sec 3a `Resolve<CallF>`/`Resolve<TypeF>` presence per language | docs sec 3a | go both, kotlin both, rust both, ts both, python both, prolog CallF only, markdown TypeF only | `src/lang/go.rs:2932,4136`, `kotlin.rs:1728,1807`, `rust.rs:547,1010`, `ts.rs:4205,4748`, `python/_0_source.rs:2637,2838`, `prolog/_0_source.rs:1115`, `markdown/_0_source.rs:494` |
| sec 5b per-language df kind counts (rust 16, ts 13, go 14, python 13, kotlin 10, prolog 8, gdscript 0, commonlisp 0) | docs sec 5b | distinct `DfNodeKind::` literals per language: rust 16, ts 13, go 14, python 13, kotlin 10, prolog 8, gdscript 0, commonlisp 0 | counted over `src/lang/**` |
| sec 5c per-language entity counts (rust 6+TRAIT, ts 7, go 5, kotlin 4, python 4+MODULE, prolog 1) | docs sec 5c | rust core 6 + `Ext`; ts 7; go 5; kotlin 4; python core 4 + `Ext` (`pub const MODULE: TypeEntityKind = TypeEntityKind::Ext(...)` at `python/_0_source.rs:46`); prolog 1 | counted over `src/lang/**`; `python/_0_source.rs:46` |
| sec 5c per-language type-edge counts (ts 7, rust 5 via `rust_type_edges.rs`, python 5, go 4, kotlin 4) | docs sec 5c | ts 7 (`Field Generic Impl Param Returns Uses Variant`), `rust_type_edges.rs` 5 (`Field Generic Impl Uses Variant`), python 5, go 4, kotlin 4 | counted over `src/lang/**` |
| sec 5d cfg nodes `Entry Exit Stmt Branch Loop Jump Ret`, edges `Next Arm Jump Exit`, six role tables, none for commonlisp/gdscript/markdown/data | docs sec 5d | exact enum bodies; `roles_for` matches rust, go, ts, kotlin, python, prolog only | `src/types.rs:1398-1433`; `src/cfg.rs:85-199` |
| sec 5a ModuleF collapse "decision 2026-08-17" at `types.rs:1563-1578` | docs sec 5a | "COLLAPSED BY DECISION" + "Fork C, chosen by Chris 2026-08-17 (issue extract-modulef-collapse)" | `src/types.rs:1565-1566,1571-1580` |
| sec 5a holes: ts `export *` fan-in dropped at `ts_resolve.rs:507`, re-export cycles never cached at `:598`; go last-segment guess at `go_modules.rs:14`; kotlin same-package ambiguity at `kotlin_modules.rs:9-11`; python supplied-set-only at `_2_modules.rs:2` | docs sec 5a | all five comments are at those lines with that content | those lines |
| sec 2 anchors `Family:173`, `FamilyBundle:1635`, `ResolutionOrigin:1660`, `UnresolvedReason:753`, `Source::extract:2713`, `Resolve:2353`, `ProjectCx:1879`, `FlatFact:3006`, `Mode` `tsi/types.rs:13`, `TierDecline` `project.rs:178` | docs sec 2 | all ten land on the named item | those lines |
| sec 4a `bin/extract.rs:483 main`, `dispatch.rs:48` | docs sec 4a | exact | `src/bin/extract.rs:483`, `src/dispatch.rs:48` |
| sec 4c `scip_ensure.rs:830 detect`, `run_capped :556`, `index.scip :431`, `scip_decode.rs:32 load_index` | docs sec 4c | exact | those lines |
| sec 4d `0_move.rs:74 run`, `Plan::build :256`, `move_stage.rs:42 stage_and_commit` | docs sec 4d | exact | `src/0_move.rs:74,256`, `src/move_stage.rs:42` |
| sec 4d `Rehome` = 4 fns, `Rename` = 3 fns | docs sec 4d | `Rehome`: `import_refs`, `respell`, `directory_stem`, `moved_names`; `Rename`: `symbol_refs`, `respell_symbol`, `text_spellings` | `src/types.rs:2776-2808`, `:2958-2981` |
| sec 3d anchors `tests/90_mutation_battery.rs:389-402` (F1), `:379-386` (F5), `:24-27` (header) | docs sec 3d | F1 comment+test at 389-395, F5 comment+test at 379-386; the rust/python/ts origins comment block is at 26-29 (cited 24-27) | `tests/90_mutation_battery.rs` |
| sec 3d `src/lang/markdown/_0_source.rs:142` `FamilyMask::ALL` yields `types: None`; sec 3a "type `p` (doc nodes, only when cst masked off)" | docs sec 3d, sec 3a | the TypeF projection is guarded by `if mask.types && !mask.cst` | `src/lang/markdown/_0_source.rs:142-149` |
| sec 7 "no test computes fast-vs-slow accuracy today" | docs sec 7 | no test file joins `resolved_edge.caller_site_start` to `scip_occurrence.start`; no `fast_slow`/`diff` test exists (170 test files) | grep `fast_slow\|caller_site_start.*scip_occurrence` over `tests/` = no matches; `tests/*.rs` inventory |
| gate: "only the 2 known `golden_parity` failures" | plan sec 2, docs sec 1 | exactly one target failed; its two tests are `ported_facets_match_v5` and `rust_doc_parity`; both diffs are the oracle path prefix `v6/sprefa-extract/...` vs actual `crates/sprefa-extract/...` | `cargo test --features cli --no-fail-fast` exit 101; log `crates/sprefa-extract/target-review1/gate.log`; see `## Gate` |
| gate: "(173 binaries)" | plan sec 2 | 170 integration test targets; 172 `Running` lines; 173 `test result` blocks (172 targets + doc-tests) | `cargo metadata --no-deps --manifest-path crates/sprefa-extract/Cargo.toml`; `gate.log` |

## Unverifiable

| claim | why |
| --- | --- |
| sec 3b "semantic TSI rows Y" for rust/ts/go | emission code exists and was read (`project.rs:773-798`); no checker tier was run here, so no actual semantic run/coverage rows were produced |
| sec 2 "the syntax planes hold (cst, df, spans, names after 16ebd451)" | no criterion given; the CTF exercised only phase-1 plus name-match resolve |
| sec 3b "Issue `extract-fast-slow-trait-divide` AC 3 is about the projections" | the issue text is not in the worktree and was not fetched |
| plan lane 5 "`ruff_python_resolver` (Pyright's algorithm, Astral)" and `ra_ap_hir_def` DefMap sizing/licence | third-party project facts, not in the repo; no web access used |
| plan "173 binaries" for the gate | the full `cargo test --features cli --no-fail-fast` run was started; the count is 170 integration test targets from `tests/*.rs` plus lib and bin targets |
| sec 6 Joern "kotlin2cpg (kotlinc frontend)", "pysrc2cpg (own parser)", "others need deps", "incremental: per-file passes exist" | memory only; I have no local receipt and no high confidence either way. Belief: kotlin2cpg wraps the Kotlin compiler PSI (medium); pysrc2cpg uses an ANTLR-derived Python grammar rather than the CPython parser, so "own parser" is defensible (low); "others need deps" is false for pysrc2cpg (low); Joern has no incremental CPG update, "per-file passes" is the frontend parse granularity (medium) |
| sec 6 CodeQL "cross-file name binding exact for compiled languages; for JS/Python their own inference" | no local receipt; belief: exactness holds for build-integrated languages, and the JS/Python arms do their own module resolution (medium) |
| sec 6 remaining cells (TRAP tuples, Code Property Graph node/edge kinds, planes lists, taint engines, query languages, refactoring = none, extract rows) | not contradicted by anything I know; left as stated rather than confirmed (no local receipt) |

## Actual leg order

rust `Resolve<CallF>` (`src/lang/rust.rs:1010-1420`), first answer wins:

1. `Receiver` at `:1151-1154`, fed by `recv_t` (`:1077-1101`, corpus `(T, m)` impl table, trait fn, trait default). A KNOWN receiver type with no impl target stops the chain (`:1155-1158`).
2. If the site carries a qualifier and the file has a path: `ModulePlane` via `module_call` (`:1166-1180`), else `ModulePlane` via `call_name_match_in_module` (`:1181-1189`). This arm is exclusive with step 3.
3. Otherwise, in order: `SelfType` assoc (`:1191`), `SelfType` `Self::` (`:1192`), `SameFile` (`:1193-1204`), `ModulePlane` import-bound (`:1205-1214`), `CorpusUnique` (`:1215-1225`).
4. `callable` filter drops collapsed macro spans and `type X =` aliases (`:1232-1233`).
5. scip fold (`:1237-1253`): agreement keeps the name leg's origin; otherwise `ScipOverride`/`Scip` wins over the name match.
6. `Checker` override (`:1256-1285`): `CheckerResolve`; under `witness` the displaced syntax leg is emitted too.

ts `Resolve<CallF>` (`src/lang/ts.rs:4748-5010`):

1. `ModulePlane` from `import_t` (`:4834-4836`, used at `:4934-4939`).
2. `ModulePlane` from `seat_t` (imported receiver seat, `:4906-4912`, used at `:4940-4942`).
3. Receiver-bearing sites: `TypeBinding::Field` -> `Receiver` then `CorpusUnique` name match (`:4945-4951`); any other traced receiver -> `Receiver` (`:4952-4954`); no receiver -> `CorpusUnique` name match (`:4955`, mapped at `:4932`).
4. scip fold (`:4968-4979`), then `Checker` (`:4980-4999`).

go `Resolve<CallF>` (`src/lang/go.rs:4136-4566`):

1. `Some(ReceiverOutcome::Named)` -> `Receiver` (`:4227-4233`).
2. `Some(ReceiverOutcome::Inferred)` with a bound type -> `Receiver` (`:4245-4262`); `Ambiguous` -> nothing (`:4266`).
3. No receiver, `callee_path` present: `go_shadowing_receiver_target` -> `Receiver` (`:4270-4290`), else directory leg `ModulePlane` (`:4291-4303`), else corpus name match `CorpusUnique` (`:4370-4381`).
4. No receiver, no `callee_path`: multi-hop chain `AliasChain` (`:4342-4352`), else same-directory `SameFile` (`:4356-4369`), else `CorpusUnique` (`:4370-4381`), else dot-import `ModulePlane` (`:4383-4396`).
5. scip and checker folds follow the `name_t` block (`:4400-`).

## CTF reproduction

```
extract fast --sqlite target-review1/ctf18.db <18 files listed in the brief>
Wrote .../crates/sprefa-extract/target-review1/ctf18.db (338834 rows)
```

```
sqlite3 ctf18.db "SELECT caller_path, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1"
src/bin/extract.rs|2
src/cfg.rs|5
src/lang/astgrep.rs|2
src/lang/extract_lang.rs|1
src/lang/go.rs|22
src/lang/kotlin.rs|17
src/lang/rust.rs|37
src/lang/ts.rs|27
src/types.rs|3
src/wire.rs|39
```

```
sqlite3 ctf18.db "SELECT * FROM resolved_edge WHERE callee_name='project'"
_row|_input_path|_content_id|record|fact|caller_path|caller_name|callee_path|callee_name|caller_site_start|caller_site_end|kind|resolution_origin
330317|None|None|resolved_edge|None|src/lang/astgrep.rs|extract|src/lang/astgrep.rs|project|10642|10649|name_resolve|same_file
330318|None|None|resolved_edge|None|src/lang/astgrep.rs|closure@10426|src/lang/astgrep.rs|project|10642|10649|name_resolve|same_file
332370|None|None|resolved_edge|None|src/lang/ts.rs|extract|src/lang/ts.rs|project|158253|158260|name_resolve|same_file
332371|None|None|resolved_edge|None|src/lang/ts.rs|closure@158042|src/lang/ts.rs|project|158253|158260|name_resolve|same_file
332375|None|None|resolved_edge|None|src/lang/ts.rs|extract|src/lang/ts.rs|project|159353|159360|name_resolve|same_file
332381|None|None|resolved_edge|None|src/lang/ts.rs|extract|src/lang/ts.rs|project|160475|160482|name_resolve|same_file
332385|None|None|resolved_edge|None|src/lang/ts.rs|extract|src/lang/ts.rs|project|161164|161171|name_resolve|same_file
```

Supporting breakdown, not part of the brief's two queries:

```
sqlite3 ctf18.db "SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1"
corpus_unique|118
same_file|30
receiver|7

sqlite3 ctf18.db "SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1"
same_file|1610
corpus_unique|813
receiver|434
self_type|206
module_plane|177

sqlite3 ctf18.db "SELECT _input_path, span__start, callee, callee_path FROM site WHERE callee='project'"
src/lang/astgrep.rs|10642|project|None
src/lang/go.rs|112220|project|None
src/lang/kotlin.rs|68544|project|None
src/lang/rust.rs|131448|project|None
src/lang/ts.rs|158253|project|None
src/lang/ts.rs|159353|project|None
src/lang/ts.rs|160475|project|None
src/lang/ts.rs|161164|project|None
```

## Gate

| measure | value |
| --- | --- |
| command | `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` with `CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-review1` |
| exit | 101 |
| integration test targets (`cargo metadata --no-deps`) | 170 |
| targets built and run (`Running ... (target-review1/debug/deps/...)` lines) | 172 |
| `test result:` blocks (targets + doc-tests) | 173 |
| failing targets | 1, `--test golden_parity` |
| failing tests | `ported_facets_match_v5`, `rust_doc_parity` |
| failure cause | oracle path prefix: oracle rows say `v6/sprefa-extract/...`, actual rows say `crates/sprefa-extract/...`; identical value pairs otherwise |
| full log | `crates/sprefa-extract/target-review1/gate.log` |

```
error: 1 target failed:
    `--test golden_parity`
```

```
---- rust_doc_parity stdout ----

thread 'rust_doc_parity' (53404687) panicked at tests/golden_parity.rs:1877:5:
rust doc parity diff vs the v5 oracle:
  only in v5 (5): [
    "doc\tv6/sprefa-extract/tests/fixtures/rust/docs.rs::enum::Mode\t16",
    "doc\tv6/sprefa-extract/tests/fixtures/rust/docs.rs::function::make_engine\t30",
    "doc\tv6/sprefa-extract/tests/fixtures/rust/docs.rs::function::trim\t25",
    "doc\tv6/sprefa-extract/tests/fixtures/rust/docs.rs::method::Engine.mode\t38",
    "doc\tv6/sprefa-extract/tests/fixtures/rust/docs.rs::struct::Engine\t11",
]
  only in v6 (5): [
    "doc\tcrates/sprefa-extract/tests/fixtures/rust/docs.rs::enum::Mode\t16",
    "doc\tcrates/sprefa-extract/tests/fixtures/rust/docs.rs::function::make_engine\t30",
    "doc\tcrates/sprefa-extract/tests/fixtures/rust/docs.rs::function::trim\t25",
    "doc\tcrates/sprefa-extract/tests/fixtures/rust/docs.rs::method::Engine.mode\t38",
    "doc\tcrates/sprefa-extract/tests/fixtures/rust/docs.rs::struct::Engine\t11",
]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

```
---- ported_facets_match_v5 stdout ----

thread 'ported_facets_match_v5' (53404686) panicked at tests/golden_parity.rs:446:9:
[lambdas] PORTED parity diff vs v5 oracle:
  only in v5 (5):
    df_node	closure	v6/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::pairs::closure::1272	1272
    df_node	closure	v6/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::pairs::closure::1272::closure::1306	1306
    df_node	closure	v6/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::1061	1061
    df_node	closure	v6/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::798	798
    df_node	closure	v6/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::915	915
  only in v6 (5):
    df_node	closure	crates/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::pairs::closure::1272	1272
    df_node	closure	crates/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::pairs::closure::1272::closure::1306	1306
    df_node	closure	crates/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::1061	1061
    df_node	closure	crates/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::798	798
    df_node	closure	crates/sprefa-extract/tests/fixtures/ts/lambdas.ts::function::summarize::closure::915	915
Regenerate the oracle: cargo run --example v5_normalize -- crates/sprefa-extract/tests/fixtures/ts/lambdas.ts > crates/sprefa-extract/tests/fixtures/ts/lambdas.v5.jsonl
```
