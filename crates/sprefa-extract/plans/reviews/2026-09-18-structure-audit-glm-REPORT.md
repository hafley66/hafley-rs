# Lane SA: sprefa-extract structure audit, BEFORE 85e59e5d vs AFTER checkout

## Commands

| # | command |
|---|---|
| 1 | `git archive -o before.tar 85e59e5d -- crates/sprefa-extract/src && mkdir -p before && tar -xf before.tar -C before` |
| 2 | `/Users/chrishafley/projects/hafley-rs/.claude/worktrees/extract-fixes/crates/sprefa-extract/target-primary/debug/extract --help` (spans documented half-open `[start,end)`) |
| 3 | `/Users/chrishafley/projects/hafley-rs/.claude/worktrees/extract-fixes/crates/sprefa-extract/target-primary/debug/extract --schema > .audit/schema.txt` |
| 4 | `/Users/chrishafley/projects/hafley-rs/.claude/worktrees/extract-fixes/crates/sprefa-extract/target-primary/debug/extract --family cst --file-fact crates/sprefa-extract/src/lang/markdown/_0_source.rs` (span-semantics sample) |
| 5 | `python3 audit.py` (p50/p90 nearest-rank; fn length = lines spanned by the half-open span; over 60/120 strict; counts from `record=node` only) |
| 6 | `.audit/commands.log`: one line per per-file call, argv `<EXTRACT> --family cst --file-fact <file>`, 199 lines |
| 7 | `python3 -c "import json; d=json.load(open('.audit/summary.json')); [print(f['name'], f['start_line'], f['end_line'], f['len']) for f in d['after']['per']['crates/sprefa-extract/src/lang/rust_receivers.rs']['fns'] if f['name']=='receiver_outcome']" && python3 -c "import json; d=json.load(open('.audit/summary.json')); [print(f['name'], f['start_line'], f['end_line'], f['len']) for f in d['after']['per']['crates/sprefa-extract/src/lang/kotlin.rs']['fns'] if f['name']=='resolve']` (spot-check aim) |
| 8 | `python3 -c "import json; d=json.load(open('.audit/summary.json')); [print(f['name'], rel.split('/lang/')[1], f['start_line'], f['end_line'], f['len']) for rel,p in sorted(d['after']['per'].items()) if '/lang/' in rel for f in p['fns'] if f['name'] in {'call_drops','call_name_match','df_edge','df_push','extract','fn_sigs','resolve','collect_receivers','field_type_of','lookup'} and rel.split('/lang/')[1] in {'go.rs','kotlin.rs','kotlin_receivers.rs','rust.rs','rust_receivers.rs','ts.rs','ts_receivers.rs'}]` (twin body aim) |
| 9 | `python3 -c "import json; d=json.load(open('.audit/summary.json')); [print(short, 'new fns:', sorted({f['name'] for f in d['after']['per']['crates/sprefa-extract/src/lang/'+short]['fns']} - {f['name'] for f in d['before']['per'].get('crates/sprefa-extract/src/lang/'+short, {'fns': []})['fns']})) for short in ['kotlin.rs','kotlin_modules.rs','rust.rs','rust_modules.rs','ts.rs','ts_receivers.rs','rust_receivers.rs']]"` |
| 10 | `diff before/crates/sprefa-extract/src/lang/mod.rs crates/sprefa-extract/src/lang/mod.rs; diff before/crates/sprefa-extract/src/lang/go.rs crates/sprefa-extract/src/lang/go.rs` |
| 11 | `diff before/crates/sprefa-extract/src/types.rs crates/sprefa-extract/src/types.rs` |
| 12 | `read crates/sprefa-extract/src/lang/rust_receivers.rs:405-412,468-472` (fn-length spot check: 405 opens `fn receiver_outcome`, 472 closes it) |
| 13 | `read crates/sprefa-extract/src/lang/kotlin.rs:1738-1743,1815-1818` (spot check: `resolve` 1738-1818) |
| 14 | `read crates/sprefa-extract/src/lang/go.rs:443-472,2689-2715,2740-2769,3014-3043,4137-4166,4556-4565,4796-4815,4866-4876` (twin bodies) |
| 15 | `read crates/sprefa-extract/src/lang/kotlin.rs:440-469,1562-1588,1615-1644,1704-1734,1738-1767,1809-1818,1900-1916,2074-2093,2120-2129` (twin bodies) |
| 16 | `read crates/sprefa-extract/src/lang/rust.rs:299-321,714-721,3251-3271,3371-3400,1011-1040,1311-1320,1370-1389,1449-1458` (twin bodies) |
| 17 | `read crates/sprefa-extract/src/lang/ts.rs:640-668,3702-3721,4057-4086,4206-4235,4287-4296,4334-4353,4399-4408,4489-4519,4765-4794,5128-5137` (twin bodies) |
| 18 | `read crates/sprefa-extract/src/lang/kotlin_receivers.rs:48-52,111-141,517-519` (twin bodies) |
| 19 | `read crates/sprefa-extract/src/lang/rust_receivers.rs:316-318,587-609` (twin bodies) |

## Totals

| metric | BEFORE | AFTER | delta |
|---|---|---|---|
| files | 99 | 100 | +1 |
| bytes | 2459360 | 2509259 | +49899 |
| fns | 2102 | 2148 | +46 |
| fn p50 | 12 | 13 | +1 |
| fn p90 | 47 | 47 | 0 |
| fn max | 691 | 691 | 0 |
| fns over 60 | 123 | 126 | +3 |
| fns over 120 | 20 | 20 | 0 |
| match expressions | 868 | 888 | +20 |
| match arms | 3438 | 3501 | +63 |
| arms per match p90 | 6 | 6 | 0 |
| arms per match max | 112 | 112 | 0 |
| structs | 390 | 392 | +2 |
| enums | 112 | 113 | +1 |
| enum variants | 801 | 806 | +5 |
| traits | 15 | 15 | 0 |
| impl blocks | 281 | 285 | +4 |
| type_parameters | 209 | 212 | +3 |
| closures | 2125 | 2211 | +86 |
| ifs | 2070 | 2138 | +68 |
| macro invocations | 979 | 984 | +5 |
| impl self-type groups >1 in one file | 46 | 46 | 0 |

<!-- BEFORE dup impls: crates/sprefa-extract/src/cpg_types.rs self=CpgImportError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/1_ast_rule.rs self=AstRuleError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/2_source_query.rs self=SourceQueryError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/4_owned_region.rs self=OwnedRegionError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/extract_lang.rs self=ExtractLang n=6 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/fact.rs self=FactError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/fact.rs self=FactMatcher n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/go.rs self=GoSource n=5 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/go_checker.rs self=GoCheckerIndex n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/go_modules.rs self=GoModuleFacts n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/kotlin.rs self=KotlinSource n=5 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/markdown/_0_source.rs self=MarkdownSource n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/prolog/_0_source.rs self=PrologSource n=3 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/prolog/_1_rehome.rs self=PrologSource n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/python/_0_source.rs self=PythonSource n=5 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust.rs self=RustCallDefs<'a> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust.rs self=RustSource n=5 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust_checker.rs self=RustCheckerIndex n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust_receivers.rs self=ReceiverWalk<'a> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust_rehome.rs self=PathScan<'_> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust_rehome.rs self=RustSource n=3 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/rust_rename.rs self=Scan<'_> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts.rs self=ConstWalker<'s> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts.rs self=TsSource n=5 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts.rs self=UnresolvedWalker<'_> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts_checker.rs self=TsCheckerIndex n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts_paths.rs self=PathLiteralWalker<'a> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts_receivers.rs self=ReceiverWalker n=3 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/lang/ts_rehome.rs self=TsSource n=3 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/move_stage.rs self=Mirror n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/project.rs self=Fixture n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/project.rs self=FsBlobSource n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/project.rs self=ProjectError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/project.rs self=ResolveWithRawError<E> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/project.rs self=SourceTreeBlobSource n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/scip_ensure.rs self=IndexBudget n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/scip_rows.rs self=ScipRecords n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/trace.rs self=SummaryState n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/trail.rs self=TrailError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/tsi/ingest.rs self=IngestError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=FamilyBundle<F> n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=ParseError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=RenameStop n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=ScipDocument n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=ScipError n=2 -->
<!-- BEFORE dup impls: crates/sprefa-extract/src/types.rs self=Strings n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/cpg_types.rs self=CpgImportError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/1_ast_rule.rs self=AstRuleError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/2_source_query.rs self=SourceQueryError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/4_owned_region.rs self=OwnedRegionError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/extract_lang.rs self=ExtractLang n=6 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/fact.rs self=FactError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/fact.rs self=FactMatcher n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/go.rs self=GoSource n=5 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/go_checker.rs self=GoCheckerIndex n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/go_modules.rs self=GoModuleFacts n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/kotlin.rs self=KotlinSource n=7 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/markdown/_0_source.rs self=MarkdownSource n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/prolog/_0_source.rs self=PrologSource n=3 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/prolog/_1_rehome.rs self=PrologSource n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/python/_0_source.rs self=PythonSource n=5 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust.rs self=RustCallDefs<'a> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust.rs self=RustSource n=5 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust_checker.rs self=RustCheckerIndex n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust_receivers.rs self=ReceiverWalk<'a> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust_rehome.rs self=PathScan<'_> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust_rehome.rs self=RustSource n=3 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/rust_rename.rs self=Scan<'_> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts.rs self=ConstWalker<'s> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts.rs self=TsSource n=5 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts.rs self=UnresolvedWalker<'_> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts_checker.rs self=TsCheckerIndex n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts_paths.rs self=PathLiteralWalker<'a> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts_receivers.rs self=ReceiverWalker n=3 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/lang/ts_rehome.rs self=TsSource n=3 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/move_stage.rs self=Mirror n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/project.rs self=Fixture n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/project.rs self=FsBlobSource n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/project.rs self=ProjectError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/project.rs self=ResolveWithRawError<E> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/project.rs self=SourceTreeBlobSource n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/scip_ensure.rs self=IndexBudget n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/scip_rows.rs self=ScipRecords n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/trace.rs self=SummaryState n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/trail.rs self=TrailError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/tsi/ingest.rs self=IngestError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=FamilyBundle<F> n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=ParseError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=RenameStop n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=ScipDocument n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=ScipError n=2 -->
<!-- AFTER dup impls: crates/sprefa-extract/src/types.rs self=Strings n=2 -->
## Changed files

| file | bytes (d) | fns (d) | match (d) | arms (d) | struct (d) | enum (d) | variant (d) | trait (d) | impl (d) | generics (d) | closures (d) | ifs (d) | macros (d) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| crates/sprefa-extract/src/lang/rust_receivers.rs | 24253 (+2010) | 24 (+2) | 21 (0) | 65 (+1) | 2 (0) | 1 (0) | 2 (0) | 0 (0) | 2 (0) | 4 (0) | 16 (0) | 32 (+3) | 2 (0) |
| crates/sprefa-extract/src/lang/kotlin_modules.rs | 10756 (+2471) | 9 (+3) | 2 (+1) | 6 (+2) | 3 (0) | 0 (0) | 0 (0) | 0 (0) | 1 (0) | 0 (0) | 10 (+3) | 8 (+4) | 1 (0) |
| crates/sprefa-extract/src/lang/ts.rs | 203540 (+1619) | 155 (0) | 104 (+1) | 389 (+3) | 15 (0) | 0 (0) | 0 (0) | 0 (0) | 17 (0) | 34 (0) | 127 (+6) | 188 (+1) | 54 (0) |
| crates/sprefa-extract/src/lang/kotlin.rs | 88510 (+11823) | 59 (+7) | 26 (+6) | 104 (+15) | 1 (0) | 0 (0) | 0 (0) | 0 (0) | 7 (+2) | 5 (0) | 101 (+33) | 99 (+15) | 7 (0) |
| crates/sprefa-extract/src/lang/rust.rs | 135201 (+1079) | 79 (0) | 39 (+1) | 148 (+3) | 8 (0) | 0 (0) | 0 (0) | 0 (0) | 9 (0) | 7 (0) | 175 (+2) | 100 (+1) | 18 (0) |
| crates/sprefa-extract/src/lang/rust_modules.rs | 53038 (+541) | 45 (+1) | 24 (0) | 73 (0) | 10 (0) | 4 (0) | 15 (0) | 0 (0) | 3 (0) | 1 (0) | 56 (0) | 41 (0) | 10 (0) |
| crates/sprefa-extract/src/project.rs | 108329 (+798) | 91 (0) | 30 (0) | 75 (0) | 17 (0) | 4 (0) | 14 (0) | 0 (0) | 14 (0) | 15 (0) | 138 (-1) | 58 (+1) | 66 (0) |
| crates/sprefa-extract/src/types.rs | 156332 (+187) | 93 (0) | 32 (0) | 167 (0) | 86 (0) | 30 (0) | 217 (+1) | 13 (0) | 57 (0) | 21 (0) | 29 (0) | 32 (0) | 21 (0) |
| crates/sprefa-extract/src/lang/go.rs | 198037 (+123) | 128 (0) | 70 (0) | 220 (0) | 7 (0) | 4 (0) | 12 (0) | 0 (0) | 7 (0) | 5 (0) | 220 (0) | 220 (0) | 32 (0) |
| crates/sprefa-extract/src/lang/ts_receivers.rs | 31952 (+3269) | 28 (+1) | 19 (+1) | 48 (+3) | 2 (0) | 2 (0) | 7 (+1) | 0 (0) | 3 (0) | 2 (0) | 22 (+3) | 40 (+8) | 4 (+1) |
| crates/sprefa-extract/src/lang/mod.rs | 6864 (+26) | 6 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 3 (0) | 0 (0) | 0 (0) |
| crates/sprefa-extract/src/lang/kotlin_receivers.rs (new) | 25953 | 32 | 10 | 36 | 2 | 1 | 3 | 0 | 2 | 3 | 40 | 35 | 4 |

| file | fn p50 (B->A) | fn p90 (B->A) | fn max (B->A) | fns>60 (d) | fns>120 (d) | arms/match p90 (B->A) | arms/match max (B->A) |
|---|---|---|---|---|---|---|---|
| crates/sprefa-extract/src/lang/rust_receivers.rs | 12 -> 12 | 44 -> 44 | 57 -> 68 | +1 | 0 | 5 -> 5 | 8 -> 8 |
| crates/sprefa-extract/src/lang/kotlin_modules.rs | 14 -> 14 | 55 -> 55 | 55 -> 55 | 0 | 0 | 4 -> 4 | 4 -> 4 |
| crates/sprefa-extract/src/lang/ts.rs | 16 -> 16 | 51 -> 51 | 357 -> 373 | +1 | 0 | 7 -> 7 | 27 -> 27 |
| crates/sprefa-extract/src/lang/kotlin.rs | 11 -> 12 | 52 -> 64 | 374 -> 374 | +1 | 0 | 9 -> 9 | 12 -> 12 |
| crates/sprefa-extract/src/lang/rust.rs | 16 -> 16 | 69 -> 69 | 691 -> 691 | 0 | 0 | 8 -> 8 | 22 -> 22 |
| crates/sprefa-extract/src/lang/rust_modules.rs | 16 -> 16 | 47 -> 47 | 136 -> 137 | 0 | 0 | 4 -> 4 | 6 -> 6 |
| crates/sprefa-extract/src/project.rs | 9 -> 9 | 46 -> 51 | 302 -> 302 | 0 | 0 | 3 -> 3 | 7 -> 7 |
| crates/sprefa-extract/src/types.rs | 8 -> 8 | 23 -> 23 | 71 -> 71 | 0 | 0 | 9 -> 9 | 18 -> 18 |
| crates/sprefa-extract/src/lang/go.rs | 17 -> 17 | 54 -> 54 | 465 -> 465 | 0 | 0 | 5 -> 5 | 16 -> 16 |
| crates/sprefa-extract/src/lang/ts_receivers.rs | 17 -> 16 | 43 -> 44 | 44 -> 51 | 0 | 0 | 4 -> 4 | 4 -> 5 |
| crates/sprefa-extract/src/lang/mod.rs | 4 -> 4 | 33 -> 33 | 33 -> 33 | 0 | 0 | - -> - | - -> - |
| crates/sprefa-extract/src/lang/kotlin_receivers.rs | - -> 15 | - -> 31 | - -> 38 | 0 | 0 | - -> 5 | - -> 7 |


<!-- len change over 60: resolve lang/kotlin.rs 33 -> 81 -->
<!-- len change over 60: call_drops lang/ts.rs 59 -> 75 -->
<!-- len change over 60: resolve lang/ts.rs 357 -> 373 -->
<!-- len change over 60: resolve lang/rust.rs 298 -> 310 -->
<!-- len change over 60: receiver_outcome lang/rust_receivers.rs 57 -> 68 -->
<!-- len change over 60: call_drops lang/rust.rs 80 -> 89 -->
<!-- len change over 60: call_drops lang/go.rs 79 -> 81 -->
<!-- len change over 60: extract lang/kotlin.rs 80 -> 82 -->
<!-- len change over 60: build lang/rust_modules.rs 136 -> 137 -->
## Duplicated helpers

| name | files | twin or drift | note |
|---|---|---|---|
| call_drops | go.rs kotlin.rs rust.rs ts.rs | twin | same job, unbound call sites to ResolveDrop rows; drop reasons differ per language (go.rs:4867 External via go_modules, kotlin.rs:2098 receiver outcomes, ts.rs:4399 module ambiguity) |
| call_name_match | go.rs kotlin.rs rust.rs ts.rs | twin | last-resort corpus name match; go/kotlin/ts bodies near identical, rust.rs:714 delegates to call_name_match_in |
| collect_receivers | kotlin_receivers.rs rust_receivers.rs | twin | both walk one parse and fill sink.aux.receivers; kotlin variant also stores KtBindPlan (kotlin_receivers.rs:139) |
| df_edge | go.rs kotlin.rs rust.rs ts.rs | twin | identical one-liner pushing DfEdgeKind::Direct |
| df_push | go.rs kotlin.rs rust.rs ts.rs | twin | same push-node-with-interned-name; only span plumbing differs (go/kotlin byte spans, rust proc_macro2 plus line_starts, ts oxc) |
| extract | go.rs kotlin.rs rust.rs ts.rs | twin | Source::extract per language: ast-grep cst plus one native parse for type/call/df |
| field_type_of | kotlin.rs kotlin_receivers.rs | twin | layered, not drift: kotlin.rs:1906 dedupes corpus-wide over the plan accessor at kotlin_receivers.rs:48 |
| fn_sigs | go.rs kotlin.rs rust.rs ts.rs | twin | SigSlot param/ret emission per callable, type params excluded |
| lookup | kotlin_receivers.rs rust_receivers.rs | twin | identical scope-chain find_map over binding frames; KtBinding vs TypeBinding |
| resolve | go.rs kotlin.rs rust.rs ts.rs | twin | Source::resolve joining sites to defs via scip/module/name/receiver legs; kotlin.rs resolve grew 33 -> 81 for the receiver leg (kotlin.rs:1762) |
| (29 more names in >=2 files not read) | | | full candidate list in the comments above |
<!-- twin candidates: 39 names in >=2 of 7 lang files -->
<!-- cand call_drops: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand call_name_match: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand df_edge: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand df_push: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand extract: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand fn_sigs: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand matches: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand name: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand push_entity: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand push_sig: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand resolve: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand resolve_type_dst: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand type_edge_candidates: go.rs kotlin.rs rust.rs ts.rs -->
<!-- cand clean_block_comment: go.rs kotlin.rs ts.rs -->
<!-- cand def_span: go.rs kotlin.rs rust.rs -->
<!-- cand module_target: kotlin.rs rust.rs ts.rs -->
<!-- cand project_call: go.rs kotlin.rs rust.rs -->
<!-- cand project_df: go.rs kotlin.rs rust.rs -->
<!-- cand project_types: go.rs kotlin.rs rust.rs -->
<!-- cand scip_call_target: go.rs rust.rs ts.rs -->
<!-- cand collect_receivers: kotlin_receivers.rs rust_receivers.rs -->
<!-- cand df_loop_row: rust.rs ts.rs -->
<!-- cand enclosing_named_def: rust.rs ts.rs -->
<!-- cand field_type_of: kotlin.rs kotlin_receivers.rs -->
<!-- cand insert: kotlin_receivers.rs rust_receivers.rs -->
<!-- cand lookup: kotlin_receivers.rs rust_receivers.rs -->
<!-- cand module_specifiers: rust.rs ts.rs -->
<!-- cand parse_jsdoc_tags: kotlin.rs ts.rs -->
<!-- cand push_candidate: go.rs ts.rs -->
<!-- cand scope_insert: go.rs ts_receivers.rs -->
<!-- cand scope_lookup: go.rs ts_receivers.rs -->
<!-- cand seed_params: rust_receivers.rs ts_receivers.rs -->
<!-- cand to_span: ts.rs ts_receivers.rs -->
<!-- cand visit_arrow_function_expression: ts.rs ts_receivers.rs -->
<!-- cand visit_call_expression: ts.rs ts_receivers.rs -->
<!-- cand visit_expr_closure: rust.rs rust_receivers.rs -->
<!-- cand visit_function: ts.rs ts_receivers.rs -->
<!-- cand visit_item_fn: rust.rs rust_receivers.rs -->
<!-- cand visit_variable_declarator: ts.rs ts_receivers.rs -->
## Judgement

| file | what grew | structure or mass | evidence |
|---|---|---|---|
| crates/sprefa-extract/src/lang/kotlin.rs | +11823 B, +7 fns, +6 matches, +15 arms, +33 closures, +15 ifs, +2 impls; resolve 33 -> 81 | both: 2 new impl blocks (structure), the rest mass | KotlinSource impls 5 -> 7; new fns receiver_target, def_in_file, field_type_of, module_target, module_type_file, type_is_corpus, call_drops; fn p90 52 -> 64 |
| crates/sprefa-extract/src/lang/kotlin_receivers.rs (new) | whole file new: 25953 B, 32 fns, 2 structs, 1 enum, 3 variants, 2 impls, 10 matches, 36 arms, 40 closures | structure (new receiver module) | absent at 85e59e5d; fns stay small (p90 31, max 38) |
| crates/sprefa-extract/src/lang/rust.rs | +1079 B, +1 match, +3 arms, +2 closures, +1 if; resolve 298 -> 310, call_drops 80 -> 89 | mass | 0 new fns, 0 type-kind deltas; fns 79 unchanged |
| crates/sprefa-extract/src/lang/rust_modules.rs | +541 B, +1 fn (is_impl_known); build 136 -> 137 | mass | no type-kind deltas; fn max 136 -> 137 |
| crates/sprefa-extract/src/lang/ts.rs | +1619 B, +1 match, +3 arms, +6 closures; call_drops 59 -> 75, resolve 357 -> 373 | mass | 0 new fns, 0 type-kind deltas; fns>60 +1 |
| crates/sprefa-extract/src/lang/ts_receivers.rs | +3269 B, +1 fn (load_type_params), +1 variant, +3 closures, +8 ifs; fn max 44 -> 51 | mass | only type delta is enum_variant +1 |
| crates/sprefa-extract/src/lang/kotlin_modules.rs | +2471 B, +3 fns (import_target, package_of, package_scope), +3 closures, +4 ifs | mass | no type-kind deltas; fn max 55 unchanged |
| crates/sprefa-extract/src/lang/rust_receivers.rs | +2010 B, +2 fns (visit_expr_call, visit_expr_closure), +3 ifs; receiver_outcome 57 -> 68 | mass | fns>60 +1; no type-kind deltas |
| crates/sprefa-extract/src/types.rs | +187 B: new enum variant Shadowed (types.rs:668) + 2 struct fields (types.rs:3336) | structure, one variant | enum_variant +1, all other counted kinds 0 |
| crates/sprefa-extract/src/project.rs | +798 B, closures -1, ifs +1 | mass | fns 91 unchanged, type-kind deltas 0 |
| crates/sprefa-extract/src/lang/go.rs | nothing: bytes +123, every count and fn stat unchanged | neither | 3 match sites widened for ReceiverOutcome::Shadowed (go.rs:3696, 4266, 4825) |
| crates/sprefa-extract/src/lang/mod.rs | +26 B, all counts 0 | wiring for the new module | diff adds one line: `pub mod kotlin_receivers;` |
## Blocked

