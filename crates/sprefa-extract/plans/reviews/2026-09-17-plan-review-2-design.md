# Adversarial review 2 of 3: lanes 1, 2, 3

Scope: design only. Plan facts are assumed true. Every objection carries an input, a receipt, or both. Probe runs are mine, on this worktree, at the time of writing.

## Design defects

Ranked worst first.

| lane.step | defect | concrete input that shows it | consequence | fix or replacement |
| --- | --- | --- | --- | --- |
| 1.2 | The `role READ` filter selects almost nothing: scip indexers do not set READ bits on references | `src/1_rename_verify.rs:246-258` measures it in-tree: "MEASURED, scip-typescript 0.4.0 ... Every reference occurrence therefore carries NO bits, IMPORT included"; committed fixture `tests/fixtures/scip_rel/expected.jsonl:9` is a reference occurrence with `"roles":0,"read_access":false` | Every fast edge joins to no slow occurrence, so all of them read as false positives and all slow occurrences read as misses. 1.3 and 2.6 then pin a 0/0/0 fiction as the accuracy story for every later lane | Delete the role predicate. Use the predicate set the crate already uses: not DEFINITION, not IMPORT, not a `local ` symbol, identifier at the span, `followed_by_paren` |
| 1.2 | The join predicate is unspecified, and span equality is wrong for 3 of 5 languages | measured site spans from `extract fast` over `tests/fixtures/ts`: `Math.sqrt` is a 9 byte site while scip's occurrence is `sqrt` (4 bytes); `new Vec2(this.x, this.y)` is a 24 byte site. `go.rs:963-990` sets the go selector site to `go_node_span(func)` = `recv.M`. `rust.rs:2079` sets the qualified-call site to `call.func.span()` = `Type::method`. `python/_0_source.rs` sites read `w.draw` (6 bytes, receiver included) | Each affected true edge is counted twice and wrongly: once as a fast false positive and once as a slow miss. Under an intersection join with no text filter, `Math.sqrt` also matches the `Math` occurrence, so one edge becomes two | Specify containment plus `content[start..end] == callee` plus first-by-(start,end), which is exactly `site_occurrence` (`scip.rs:917-956`, whose own comment says it "filters the receiver/path occurrences (`Math` in `Math.sqrt`)") |
| 1.2 | Target comparison is impossible from the flat row being joined | `src/types.rs:3320-3334`: `resolved_edge` carries `caller_path`, `caller_name`, `callee_path`, `callee_name`, `caller_site_start/end`, and the doc says the target "travels as a path plus a name, never a nested span join". Probe over `crates/sprefa-extract/src` (73 `push` edges) binds every `sink.nodes.push(..)` to the same-file `push` def at `rust.rs:81628` | A wrong-target edge at a right site is uncountable, so the table cannot distinguish the CTF's false `push` binding from a true one. 1.1 adds spans to `scip_def`/`scip_ref`/`scip_fn_edge` and never to `resolved_edge`, which is the side that needs it | Add `to_blob`/`to_start`/`to_end` to `ResolvedEdge` (`ProjectEdge` already carries them, `types.rs:3300-3305`), or grade at (blob, def-name) as the existing ratchet does |
| 1.2 | One call site produces N fast rows, one per covering def, and the table counts rows | probe: 1966 duplicate (caller_path, site span, callee) groups in `fast.db`; `astgrep.rs` site 10642 appears twice, as caller `extract` and caller `closure@10426` | Every site inside a closure or a spliced macro body double counts in both the true and the false column, so the origin table is a table of rows, not of edges | Fix the edge key as (path, site span, target) and dedup, or iterate `call.aux.sites` per site as `golden_parity.rs:1046-1052` already does |
| 1.2 | The two databases are uncoordinated: no revision binding, and path coordinates depend on the invocation | Plan section 2 runs `extract fast --sqlite f.db <files>` and `extract slow --sqlite s.db .`; scip document paths are indexer-root-relative (`golden_parity.rs:1035-1040` matches `doc.relative_path` against paths relative to the indexer root, not the repo root), while fast paths are the argv strings. Any edit between the two runs shifts every span | A zero-row join prints a clean 0/0/0 table, which satisfies "asserts the table shape" and pins 0 false positives: silent green. Cross-run span drift has the same shape | Canonicalize both sides, join on `_content_id` (already written per row by the sqlite writer), refuse when a file's content id differs, and assert non-zero coverage per language |
| 1.2 | External calls are counted as misses | scip emits a READ occurrence for `Vec::push` or `String::lines`; fast emits no edge because no corpus def exists; `definition_of` returns None for a symbol "with no definition in the indexed corpus (an EXTERNAL)" (`scip.rs:957-959`) | The miss column is dominated by std and library calls, and 2.6 pins that number as an accuracy ratchet | Reuse the existing leg 6 rule (`golden_parity.rs:1160-1162`, `:1249-1250`): a site scip resolves externally is counted, not a miss |
| 1.1 | Redundant spans on symbol-only projections | `scip_occurrence` already carries byte spans; the matrix's own correction says so. A symbol referenced 5 times in one file yields 1 `scip_ref` row today | Adding per-occurrence spans turns those projections from per-(file, symbol) into per-occurrence, multiplying rows and creating a second copy of data the diff joins to anyway | Drop 1.1. Join the diff to `scip_occurrence` directly |
| 1.3 | It is a duplicate of three tests that already run the real indexers and already assert more | `tests/golden_parity.rs:946` `call_resolve_scip_ratchet_ts`, `:1257` `..._go`, `:1555` `..._rust`: each builds scip-typescript, scip-go or rust-analyzer over the committed fixtures, builds the in-process `Resolve`, joins with `site_occurrence` + `definition_of` + `containing_def_site`, and asserts missing_occurrence 0, disagreements 0, overbound 0 (`:1188-1210`), counting externals instead of failing them | A fourth copy over the same three fixture sets pins the same numbers twice, and a later lane (2.6) edits the same new file a second time, so the ratchet becomes a rubber stamp | Delete 1.3. Add the one axis the existing ratchets lack: an origin-keyed counter (`RatchetCounts` is keyed by kind, `golden_parity.rs:1828-1837`). Pin floors in the RATCHET.tsv plus `RATCHET_BUMP` discipline (`tests/ratchet_recall.rs:1-12`) |
| 2.5 | It demotes the leg that is not producing the false positives the plan measures, and its premise is inverted | probe over `crates/sprefa-extract/src`: 73 `push` edges, all 12 sampled rows `resolution_origin = same_file`; all 14 sampled `corpus_unique` rust edges are member calls (`.restore()`, `.flatten()`, `.with_relocate_mod()`, `.values()`, `.lines()`, `.replace()`, `.batch()`, `.get()`), zero free calls | 2.6's "false positives must drop to 0" is unreachable by a corpus_unique-only demotion, because `same_file` (3900 of 8080 rust edges in the probe) answers the same sites. "corpus_unique only answers free calls" is the inverse of the measured population | Write the rule per receiver shape, not per leg: an untyped receiver loses every name-match leg including `same_file`. Re-derive the acceptance number from measured origins, not from the leg name |
| 2.5 | The premise is also contradicted by the crate's own survey, so the criterion is untestable as written | `tests/90_mutation_battery.rs:24-27`: "rust has no duplicate-def row: no rust leg answers `corpus_unique` (survey: same_file/module_plane/receiver/self_type only)"; the code (`rust.rs:1214-1222`) and the probe (1071 rust `corpus_unique` rows) say otherwise | A lane that keys a ratchet on "corpus_unique" cannot be verified against a survey that says the origin does not exist for rust | Fix the survey or the leg list first; state the lane against a measured per-leg histogram |
| 2.5 | Wrong reason vocabulary | `types.rs:757-763`: `ambiguous` is "the corpus defines the name and this tier cannot say which one is meant"; `inferred` is "a receiver type this tier declines to trace". Probe over the crate: `text.lines()` binds a single corpus `lines` def, so there is no ambiguity | The `ambiguous` histogram already holds 2743 rows against 8674 `inferred`; mislabeling unknown receivers as ambiguous makes downstream programs treat std-member noise as corpus ambiguity | Unknown receiver type emits `inferred`; no corpus def of that name emits `no_corpus_def`; `ambiguous` only for 2 or more corpus candidates |
| 2.4 | The scope key is unspecified and a name-keyed block over-blocks nested scopes | The plan's own example is a closure param in `wire.rs`. python's existing check is `shadowed` (`python/_0_source.rs:3410-3416`), keyed `p.def.start <= site.start` (enclosing def only); `Param`/`LetBind` nodes carry only the binding's own span | A closure param named X blocks calls to X for the whole enclosing def, deleting true edges, and it will also block the module_plane and receiver legs that 2.3 and 2.2 add unless it gates them too | Block within the innermost enclosing scope span (`DfF` `Closure` node span, else the def span), and gate every name-match leg, not just `corpus_unique`/`same_file` |
| 3.1 | The trait is a table rename, and the third provider already lives inside provider 1 | The checker tier writes into the same `resolved_edge` table under `ResolutionOrigin::Checker`: `ts.rs:4991-4993`, `go_checker.rs:302-305`, `rust.rs:1282-1285` | `FastProvider` is "the resolved_edge table plus the checker rows that are already in it"; the split is not fast vs slow but site-answer vs symbol-index | Keep one neutral relation and one origin column; delete the trait |
| 3.1 | The trait shape does not hold for the checker provider | The checker is site shaped: `index.call_at(path, site.span, callee)` (ts.rs:4985), per-site answers built by `TsCheckerIndex::build` / `RustCheckerIndex::build` (`project.rs:830-943`), and it declines loudly when its tool is missing (`TierDecline`, `project.rs:178`) | A `defs`/`refs`/`imports` enumeration cannot be answered by a per-site checker without walking every site, so the third impl is either O(corpus x sites) or empty | Model the checker as a site answer (`fn answer(path, span, callee) -> Option<DefId>`) if any trait is kept at all |
| 2.3 | Module plane first outranks a leg that is right today | `use crate::util;` plus `fn run(util: &CstProjector) { util.project(&p); }`: today `recv_t` wins (`rust.rs:1151-1157`, origin `Receiver`); after 2.3 the module plane reads the receiver's name as a module and binds `util::project` | A correct edge becomes a wrong edge. Rust keeps values and modules in different namespaces, so a value binding must beat a module path | Order the receiver's binding kind before the module plane, or make the module plane decline when the qualifier is a value binding in the file's scope |
| 1.2, 3.2 | A program-shaped analysis is added as a CLI verb and two permanent planes | Boundary law: "PROGRAMS over already-emitted facts (zero new extraction - do NOT add extractor code for these)". The one in-crate derived relation, `scip_file_edges`, is admitted only under a measured-amplification argument (`scip_rows.rs:340-370`) | An accuracy meter over two databases is a metric program. The three existing ratchets show where it belongs | Keep 1.2 inside `tests/` beside the three ratchets. If a CLI verb ships anyway, it owes the same measured-amplification justification `scip_file_edges` carries |

## Misses created by lane 2.5

Every row is a call that resolves today only through a name-match leg that 2.5 removes. "Today via" is measured where a probe covers it, otherwise read from code.

| language | call shape (code) | resolves today via | recoverable by deterministic leg? which? |
| --- | --- | --- | --- |
| rust | `sink.nodes.push(node)` (crate's own `rust.rs`) | `same_file` (measured: 73/73 `push` rows), not corpus_unique | No. Field chain on an inferred local; the honest reason is `inferred`. Note this is a false edge today, so 2.5 does not fix it |
| rust | `text.lines()` (`0_rename.rs` `read_rename_list`) | `corpus_unique` (measured) | No. std method on `String`; reason must be `no_corpus_def`, not `ambiguous` |
| rust | `cx.batch()` where `cx: &MoveCx` and the field has a declared type (measured `0_rename.rs` `build -> batch` twice) | `corpus_unique` (measured) | Yes. Receiver-typed leg through `df_field` and `sig{owner, slot}`; the data is emitted, the leg is not on 2.3's list for member calls |
| rust | `journals[index].restore()` (measured `0_move.rs`) | `corpus_unique` (measured) | No. Element type of a `Vec` field needs inference; reason `inferred` |
| rust | `fn run<T: Repo>(r: T) { r.load() }` | `corpus_unique` when `load` is corpus unique | Yes in principle: trait-bound leg through `Generic`/`Param` type edges plus `IfaceImpl`; absent from 2.3's rust list |
| rust | closure param: `xs.iter().for_each(\|push\| push(1))` (the plan's own `wire.rs` example) | `corpus_unique` / `same_file` | No. Closure param types need inference; reason `inferred`, and it is also the over-blocking case in defect row 12 |
| ts | `items.map(it => it.save())` | `corpus_unique` (measured origin population: ts probe returns only `corpus_unique`) | No. Arrow param has no annotation; reason `inferred` |
| ts | `const { project } = projector; project.call()` (destructured receiver: `ts_receivers.rs:236-256` seeds `TypeBinding::Field`) | `Receiver` when `member_in` finds the member, else `corpus_unique` (`ts.rs:4946-4959`) | Yes for declared members: extend the destructure seed through the base type's fields. No for dynamic ones |
| ts | `foo.bar.load()` where `foo.bar` is not a declared field | `corpus_unique` with the member flag (`ts.rs:4932`) | No without a checker; reason `inferred` |
| python | `self.load(k)` inside the class body | `corpus_unique`; the param leg skips `self`/`cls` (`python/_0_source.rs:226`, `:1003`), and python mints no `same_file` call edge (probe: origins are `corpus_unique` plus `alias_chain` only) | Yes in principle: an enclosing-class leg. python has no `SelfType` producer (`method_owner` is rust only), so 2.3's `self_type` slot is empty for python |
| python | `w.draw()` where `w = Widget()` (measured `corpus_9.py`) | `corpus_unique` (measured) | Partly: a binding leg from the constructor call result. python has `ReturnCall` for `f()()` only, not for `x = f(); x.m()` |
| python | `helper()` after `from m import *` with a later local `def helper()` | `corpus_unique`; no `same_file` call leg exists | Yes: a same-file leg for python, which the mutation battery header says python does not mint today |
| go | `x := NewFoo(); x.Bar()` | `corpus_unique`; the go receiver leg declines `:=` bound to a call result (`go.rs:4235-4245`, and `UnresolvedReason::Inferred` documents the shape) | Yes: constructor-return-type leg from `sig{owner, slot=ret}`; not on 2.3's list |
| go | `var reg = NewRegistry(); reg.Add(1)` at package scope | `corpus_unique` | Yes, same leg as above through the package-scope var binding |
| kotlin | cross-file same-package `helper()` with no import | `corpus_unique` (matrix 5a: kotlin resolve never reads `KtModuleIndex`) | No: `kotlin_modules.rs:8-11` states a name two files of one package both declare is ambiguous and binds nothing, and the plane reads import headers only. Needs a package-scope index, which 2.1 does not add |

## Module-plane-first wrong answers

Rows 1 to 4 are regressions introduced by 2.3. Rows 5 to 7 are wrong answers the plane already produces or would produce where 2.2 and 2.3 put it first.

| language | input | module plane says | correct answer | why |
| --- | --- | --- | --- | --- |
| rust | `use crate::util;` then `fn run(util: &CstProjector) { util.project(&p); }` | `crate::util::project` | `CstProjector::project` | `util` is a value binding; Rust resolves value paths against values first. Today `recv_t` runs before the module plane (`rust.rs:1151-1157`) and is right |
| rust | `use crate::wire::push;` then `fn f(push: fn(u32)) { push(1); }` | `wire::push` | the local param binding, no corpus call edge | 2.4 blocks only `corpus_unique` and `same_file`, so the module plane answer survives the shadowing rule |
| go | `import . "pkg"` then `func f() { Helper := func(){} ; Helper() }` | `pkg.Helper` | the local variable | go.rs already encodes the correct rule in the receiver leg: "A local shadowing the package name wins; the directory leg is exported-only, corpus-wide name match last" (`go.rs:4285-4300`). 2.3 puts the module plane ahead of that check |
| kotlin | `import a.b.run` plus a local `val` or param named `run` used as `run()` | `a.b.run` | the local binding | Kotlin gives file and local declarations precedence over imports; `kotlin_modules.rs` reads import headers only, so it cannot see the local |
| python | `from m import *` then a later `def helper()` in the same file, then `helper()` | `m.helper` | the file-local `def helper` | A later module-level definition overwrites a star-imported name. python mints no `same_file` call edge, so nothing else answers |
| python | `import x as z` then `def f(z): return z()` | `x` | the parameter | `tests/90_mutation_battery.rs:389-402` (F1) already documents this as a defect with `corpus_unique` as the wrong leg. With 2.3 it becomes the module plane, and 2.4's stated scope does not cover it |
| python | `from m import *` where `m.__all__ = ["a"]` and `m` also defines `_b`, then `_b()` | `m._b` | unresolved (star imports skip `_` names and honor `__all__`) | `PyModuleIndex`'s star arm has no `__all__` and no underscore filter (matrix 5a lists no such reading) |

## Lane 3 verdict

| question | answer | receipt |
| --- | --- | --- |
| Is `trait FactProvider` a real abstraction or a table rename? | A table rename with two dead methods. `defs`, `refs` and `imports` have no consumer in the plan (the diff is `--relation call`), and each impl returns "the same row types", which requires the neutral row structs 3.2 defines anyway | Plan 3.1, 3.2, 3.3; `src/5_diff.rs` does not exist yet |
| What does a third provider look like (the checker tier)? | It already exists as rows inside provider 1. All three checker arms overwrite or mint `resolved_edge` rows under `ResolutionOrigin::Checker` | `ts.rs:4991-4993`, `go_checker.rs:302-305`, `rust.rs:1282-1285`, `project.rs:755-760` (the TSI run row reads the origin) |
| Does the trait shape hold for it? | No. The checker is site shaped, not enumeration shaped: `index.call_at(path, site.span, callee)`, per-site answers from `TsCheckerIndex::build`/`RustCheckerIndex::build`, and it declines loudly when its tool is missing | `ts.rs:4985`, `project.rs:830-943`, `TierDecline` `project.rs:178` |
| If the trait is unnecessary, what replaces it? | Nothing, or a view. The one relation the diff needs is per call occurrence: (path, site span, callee, resolved def or symbol, provider). Fast already writes its half through `resolved_edge` plus the target span it is missing, slow can project its half from `scip_occurrence` with `definition_of` plus `containing_def_site`. That is one SQL join, and it needs no materialized neutral table and no vtable | `scip.rs:917-959` (`site_occurrence`, `definition_of`), `golden_parity.rs:1057-1080` (the same join done in-process, for free) |
| Does 3.2's "both emit the neutral tables alongside their native rows" cost anything? | Yes: a second source of truth per database, in a producer that already writes 969,690 rows for 40 rust files, with drift between the native and neutral copies as the failure mode | my probe: `extract fast --sqlite` over 40 rust files wrote 969,690 rows in 15.2 s |
| Does 3.3 make 1.2 cheap? | It makes 1.2 correct, which is the ordering inversion: the plan ships 1.2 first and 3.3 then says 1.2 should read 3.2's output | plan 1.2 versus plan 3.3 |

## Law violations

Both laws quoted verbatim from `crates/sprefa-extract/AGENTS.md`.

| law line | verdict | lane.step | receipt |
| --- | --- | --- | --- |
| "Intra-procedural extraction ONLY - the per-file purity is what keeps this crate parallel and incremental." | Not violated literally: 1.2, 1.3, 2.x and 3.x add no extractor pass and no per-file context. The inter-procedural prohibition targets eager whole-repo extraction, which no lane proposes | none | AGENTS.md, "Inter-procedural rule (permanent)" |
| "Framework knowledge (hook naming conventions, RTK codegen patterns, ORM idioms) NEVER enters this crate" | None found. Every lane step is language semantics (module rules, scopes, receivers) or test scaffolding, not framework convention | none | AGENTS.md, "The boundary law" |
| "PROGRAMS over already-emitted facts (zero new extraction - do NOT add extractor code for these)" | Concern, not violation: an accuracy meter over two databases is a metric program. The in-crate precedent `scip_file_edges` is admitted only under a measured-amplification argument, and 1.2 carries none. 3.2 then promotes a program's intermediate into permanent planes | 1.2, 3.2 | AGENTS.md, "Analysis family map"; `scip_rows.rs:340-370`; three equivalent ratchets already live in `tests/` |

## Reorder proposal

The plan's order is 0, 1, 2, 3, 4. Two inversions: 1.2 cannot be written correctly before 3.1/3.2 (it needs the target span and the neutral relation its own 3.3 says it should read), and 2.5 must not land before the deterministic legs that replace it plus a meter that can see the loss.

| step | needs first | why |
| --- | --- | --- |
| 0.1 to 0.3 | nothing | docs only |
| 3.1 plus 3.2 | 0 | the neutral relation plus the target span is the cheapest place to fix defect rows 3 and 4; every later measurement reads it |
| 2.1, 2.2 | 0, independent of 3 | threading `KtModuleIndex` and `PyModuleIndex` changes no answer until 2.3 ranks them; these two are the replacements 2.5 depends on |
| 1.2 (rewritten as a join over 3.2's relation) | 3.1, 3.2 | otherwise the join is written twice, once against `resolved_edge` and once against the neutral table, and the first version cannot compare targets |
| 2.3 | 2.1, 2.2 | the module plane must answer before it is ranked first, or ranking it first just deletes the guess that used to answer |
| 2.4 | 2.3 | the shadow rule must gate the new legs too: module plane, receiver, self type and the name matches |
| 1.3 (rewritten as an origin-keyed counter inside the three existing ratchets) | 1.2 | it pins what the meter measures, and it must not be a fourth copy |
| 2.5 plus 2.6 | 2.3, 2.4, 1.3, and a per-leg miss list that is empty for the shapes in the misses table | expectations only become correct once the replacements exist and the loss is visible |
| 4.1, 4.2 | 2.1, 2.3 | go rename stops without a spelled receiver, which is the leg 2.3 adds |
| 4.3, 4.4 | 2.2, 4.1, 4.2 | the python twins reuse `PyModuleIndex` and the go arm's shape |

Proposed order: 0, 3.1 plus 3.2, 2.1 plus 2.2, 1.2, 2.3, 2.4, 1.3, 2.5 plus 2.6, 4.

Rationale in one table:

| move | change from the plan | gain |
| --- | --- | --- |
| 3 before 1 | plan has 3 last | one join implementation with a target span, instead of a span-only join rewritten later |
| 2.1 and 2.2 in parallel with 3 | plan has them inside lane 2 | no answer changes, and they are 2.5's and lane 4's prerequisite |
| 1.3 after 2.4, not inside lane 1 | plan has 1.3 inside lane 1 | the pinned numbers land after the legs that move them, so 2.6 does not edit the same file twice |
| 2.5 and 2.6 last in lane 2 | plan has them as lane 2's tail already | kept, but gated on the miss list being empty for the measured shapes |

## Receipts

Commands I ran, in order, with the number each produced. All writes went to `crates/sprefa-extract/target-review2` and no repository file was modified.

| command | result used above |
| --- | --- |
| `CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-review2 cargo build --features cli --bin extract` | built in 1m 09s |
| `extract fast --sqlite <target>/probe/fast.db` over 40 rust files under `crates/sprefa-extract/src` | 969,690 rows; origins `same_file` 3900, `module_plane` 1496, `corpus_unique` 1071, `receiver` 1063, `self_type` 550; unresolved `inferred` 8674, `ambiguous` 2743, `no_corpus_def` 2551, `external` 1567 |
| `select ... from resolved_edge where callee_name='push'` | 73 rows, all `same_file`, all bound in `rust.rs` |
| byte slices at `caller_site_start..caller_site_end` for 14 `corpus_unique` rows | all 14 are member calls on untyped receivers |
| duplicate key check: `group by caller_path, caller_site_start, caller_site_end, callee_name having count(*) > 1` | 1966 groups |
| `extract fast` over `tests/fixtures/{ts,go,python}` into `probe/{ts,go,py}.db` | ts 6 edges (all `corpus_unique`), go 5 (`corpus_unique` 4, `iface_impl` 1), python 5 (`corpus_unique` 4, `alias_chain` 1) |
| site-span slices from the `site` table per db | ts `Math.sqrt` 9 bytes, `new Vec2(...)` 24 and 34 bytes, `values.map(...).flat` 72 bytes; python `w.draw` 6 bytes, `Widget` 6 bytes; go free calls 4 bytes |

## Blocked

none.
