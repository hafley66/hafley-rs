# ryi fast-tier v5 parity receipt

Worktree: `feature/ryi-v5-parity`. Base: `a44eced8`. No merge or push. v5 source was read only. No compiler/type tier was enabled. Builds used `KACHE_DISABLED=1`; each Cargo invocation had a 110-second cap. No installs or startup builds were run.

## Commits and sites

| Item | Commit | Change sites | Targeted tests |
|---|---|---|---|
| 1. Labeled break | `53c0d594` | `crates/hafley_scm/src/lang/rust/12_df_control.rs:6`, `:100` | `labeled_break_cst_and_whole_flow` |
| 2. Call end line | `81a36197` | `crates/sprefa-extract/src/bin/ryi/0c_lines.rs:15` | `v5_call_definitions_include_the_end_line`, `whole_call_rows_include_end_lines_at_newline_boundaries`, `t_142_lines_flag` |
| 3. Owning function | `e13c9aa8` | `crates/hafley_scm/src/read/0c_flat_fact.rs:27`, `read/wire.rs:650`, `read/lang/ts/0_df_rows.rs:102`, `lang/rust/11_df_syntax_rows.rs:196` | `whole_flow_rows_name_their_owning_function`, `function_column_matches_jsonl_and_sqlite`, `t_12_df_identity`, `t_0_sqlite`, `tree_df_rows_match_snapshot_across_pinned_rust_fixtures` |
| 4. JSX lift and prop flow | `dd196df5` | `crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs:3`, `read/0d_flow.rs:61` | `whole_jsx_rows_and_resolved_prop_edges_match_v5_cases`, `t_13_flow_join` |
| 5. Seeded reach | `9862f79a` | `crates/sprefa-extract/src/0d_graph_flow.rs:4`, `src/0_graph.rs:580`, `schema/cli/ops.tsp:118` | `seeded_forward_and_reverse_reach_port_v5_dataflow_cases`, `jsx_ten_forward_and_reverse_reach_checks_match_v5`, `flow_paths_join_local_and_interprocedural_edges_with_stored_witnesses`, `t_178` |
| Deferred JSX replacement | `c41d6542` | `crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs:3`, `read/0c_arg_fields.rs:3`, `read/0c_closure_flow.rs:3`, `read/0c_call_owner.rs:5`, `read/lang/ts/4_call_closure.rs:14` | `deferred_jsx_and_generic_object_capture_flow_have_whole_outputs`; all 8 v5 parity tests; `t_13_flow_join`, `t_12_df_identity` |
| 6. Classification report | This report commit | v5 `src/engine/decls.rs:1092`, `src/engine/extract/call.rs:108`, `:368`, `src/engine/family/call_kind.rs:30` | Report only; no implementation |

New tests are grouped in `crates/sprefa-extract/tests/199_v5_parity.rs`; snapshots are in `tests/snapshots/v5_parity__*.snap`. New code files remain below 1000 lines. `src/0_graph.rs` was reduced from 1050 to 988 lines.

## Before/after rows on ported cases

### 1. v5 dataflow.rs:1116

The grammar JSON and the CST snapshot show `label` as a plain named child on both the loop and break, without a `label` field. The fix reads that child and normalizes apostrophe/colon spelling.

Before, `call_res 154:163 -> break 141:163` and `loop 99:180 -> let_bind outcome 89:96` existed, but the connection between them was absent. After:

```jsonl
{"record":"edge","family":"df","kind":"direct","from":{"start":154,"end":163},"from_kind":"call_res","to":{"start":141,"end":163},"to_kind":"break"}
{"record":"edge","family":"df","kind":"direct","from":{"start":141,"end":163},"from_kind":"break","to":{"start":99,"end":180},"to_kind":"loop"}
{"record":"edge","family":"df","kind":"direct","from":{"start":99,"end":180},"from_kind":"loop","to":{"start":89,"end":96},"to_kind":"let_bind"}
```

Node count stays 11; edge count changes from 5 to 6. The break reaches the outer labeled loop, despite the intervening inner loop. The later reach tests confirm `produce() -> outcome` in both query directions.

### 2. v5 call_golden BASE_SRC

Before/after for the `run` call-definition node:

```jsonl
{"record":"node","family":"call","span":{"start":45,"end":80,"line":5,"col":8},"kind":"function","name":"run"}
{"record":"node","family":"call","span":{"start":45,"end":80,"line":5,"col":8,"line_end":8},"kind":"function","name":"run"}
```

`writer` becomes start line 12 / end line 14; `reader` becomes 16 / 18. `line_end` is inclusive: nonempty half-open spans use byte `end - 1`, empty spans use `start`. The boundary snapshot covers an exclusive end at the following line's start and empty spans.

### 3. Owning function

Before, DF node rows omitted `function`. After, all 29 nodes in the three ownership fixtures have it, and SQLite has the same column and values:

```jsonl
{"record":"node","family":"df","span":{"start":16,"end":29},"kind":"param","name":"value","function":"format"}
{"record":"node","family":"df","span":{"start":13,"end":17},"kind":"param","name":"prog","function":"make_rels"}
{"record":"node","family":"df","span":{"start":15,"end":28},"kind":"param","name":"value","function":"outer"}
```

| Fixture | Nodes | Owning functions after |
|---|---:|---|
| v5 dataflow.rs:239, `1_widget.ts` | 9 | `format`, `Widget.render` |
| v5 dataflow.rs:578, `2_loop.rs` | 13 | `make_rels`, `work` |
| `3_closures.ts` | 7 | `outer`, `read` |

The whole JSONL snapshots retain each row. The paired SQLite snapshot selects `(span start, span end, kind, name, function)` and compares it to JSONL.

### 4. v5 flow_jsx.rs, three src/app.tsx cases and the requested deferred replacement

Before item 4, JSX expressions became opaque `expr` nodes; attribute expressions were not lifted. Commit `dd196df5` first emitted the following JSX-specific rows:

| Ported fixture | New nodes | df_field rows | jsx_prop rows |
|---|---:|---:|---:|
| `3_jsx_props.tsx` | 2 | 3 | 1 |
| `4_jsx_exprs.tsx` | 3 | 12 | 6 |
| `5_jsx_member.tsx` | 2 | 2 | 1 |

The expression case includes the array's existing `new` node and element fields. Attribute values use the existing conditional, nullish, logical, template, member, and array lifts. Non-text JSX children produce `children` fields.

Those value/target endpoints are retained by the final generic `arg_to_param` field edges, with blob identities in the full snapshots:

```text
3_jsx_props.tsx   secret 173:179 -> Card.title param 15:20
4_jsx_exprs.tsx   title value 278:300 -> Card.title param 15:20
                 subtitle 312:328 -> Card.subtitle param 22:30
                 note 337:354 -> Card.note param 32:36
                 label 364:378 -> Card.label param 38:43
                 opt 385:396 -> Card.opt param 45:48
                 items 405:420 -> Card.items param 50:55
5_jsx_member.tsx  secret 164:170 -> Panel props.title member 59:70
```

The matching-only patch and its uncommitted fixtures 13/14/15 were discarded after the coordinator withdrew them. The final replacement deletes `FlowEdgeKind::JsxProp` and its special `New` branch. A component element now lowers as an eager props object plus a closure that captures it and defers the component call. Host tags create no call or closure. The eager object is owned by the enclosing function; the captured read, call, and implicit return are owned by the closure. Attribute values retain their real input byte spans; the object covers its attributes/children and the call uses the opening-element span. No source rewriting occurs. `key` and `ref` are omitted from props fields; spreads use `..`; children use `children`.

Generic `ArgToParam` field flow follows identity reads/bindings to object values, matches destructured parameter fields, and matches member reads only through the actual positional parameter. Resolved definition spans select the callee. Destructured parameters with known object fields receive their own matching fields rather than the whole props object. Closure capture/return projections emit `lambda_elem` and `lambda_ret`, including ordinary Rust and TS closures. TS child scopes inherit outer bindings, with parameters overwriting captured names.

| Added fixture | Before replacement | After replacement |
|---|---|---|
| `16_direct_vs_jsx.tsx` | JSX used a separate `jsx_prop` kind; direct object calls had no field-value hop | `arg_to_param`: direct `x 104:105 -> title 15:20`; JSX `x 133:134 -> title 15:20` |
| `17_other_object.tsx` | Special JSX matching would target unrelated `other.title` by its name | 0 argument-field edges into `other.title`; ordinary positional flow still enters `props` |
| `18_second_scope.tsx` | Name-based JSX matching could select both `Card` parameter nodes | `x 173:174 -> top-level title 15:20`; 0 edges into the second scoped `Card` |
| `19_eager_hook.tsx` | JSX was one eager `new` node; no deferred call frame | Hook `useHook(x) 154:164` owns `function: App`; Card call `125:182` owns `function: App::closure::125`; eager object `131:185` owns `App`; closure value `125:192` owns `App` |
| `20_capture.ts` | No emitted lambda capture/return edges | `lambda_elem` and `lambda_ret` rows |
| `21_capture.rs` | No emitted lambda capture/return edges | `lambda_elem` and `lambda_ret` rows |

`--callers Card` returns the closure in the JSX cases. In the hook fixture, the Card caller is `closure@App:blake3:abbc1472489fbb1c33eb5349e67917749ac06b8d5e38a3217696269c1ce85071:0`, while `--callers useHook` returns `App`.

The final three v5 fixtures retain respectively 3, 12, and 2 `df_field` rows, and each has one deferred component `call_res` plus one closure. Their total `arg_to_param` row counts are 1, 6, and 2; the member case's two rows are positional object-to-props flow and field-value-to-member flow. Each fixture also emits one `lambda_elem` and one `lambda_ret` row.

The whole-output snapshots pin syntax rows, resolved flow, and callers. A compact whole-output proof snapshot pins the direct/JSX target arrays, zero member targets, scope isolation, and both lambda edge kinds.

Replacement commit line counts (`git diff c41d6542^ c41d6542 --numstat`):

| Group | Deleted | Added |
|---|---:|---:|
| Production Rust | 174 | 477 |
| Tests and fixtures | 1 | 158 |
| Moved allowlist entries | 3 | 3 |
| Whole-output snapshots | 777 | 5169 |
| Total | 955 | 5807 |

Allowlist entries move with the existing TS DF helpers and Rust snapshot test module; counts are retained. The new helper name avoids adding a fourth `inside` definition. The registration rail now includes declared Cargo test targets and their modules, including this standalone parity suite.

### 5. v5 seeded reach cases

The previous graph walker already combined local DF and cross-function edges. Forward traversal of stored `ret_to_call_res` edges used the storage direction, and reverse queries had no flag. The new query swaps only those return-edge endpoints before traversal; `--reverse` then swaps every edge and uses the same existing `paths` / `first_discovery` walk. Seeds are demanded individually; no all-pairs closure is computed.

```sh
ryii graph --flow-path FILE@START:END PATH...
ryii graph --flow-path FILE@START:END --reverse PATH...
```

The snapshot retains complete `graph_path` rows with depth and witness IDs for each query.

| v5 case / source -> target | Forward after | Reverse after |
|---|---|---|
| :140 Rust `name` param -> `u` read | true | true |
| :239 `Widget.render` name param -> label binding | true | true |
| :341 Rust q param -> m read through binop | true | true |
| :341 TS q param -> m read through binop | true | true |
| :925 produce() -> x | true | true |
| :925 first fallback() -> x | true | true |
| :925 second fallback() -> y | true | true |
| :925 pick(k) -> y | true | true |
| :1029 produce() -> outcome | true | true |
| :1029 fallback() -> outcome | true | true |
| :1116 labeled produce() -> outcome | true | true |
| Additional callee identity value param -> caller result binding | true | true |

The local chains were already forward-reachable, except the original labeled-break connection fixed in item 1. The additional callee-to-caller case establishes the corrected return-edge direction. Before item 5, reverse command rows were unavailable.

The ten v5 flow_jsx.rs:137 checks, each run forward from named App reads and backward from the Card parameter:

| Source -> Card prop | Forward after | Reverse after |
|---|---|---|
| secret -> title | true | true |
| fallback -> title | true | true |
| secret -> subtitle | true | true |
| backup -> subtitle | true | true |
| secret -> note | true | true |
| secret -> label | true | true |
| bag -> opt | true | true |
| first -> items | true | true |
| secret -> items | true | true |
| guarded -> note | false | false |

Before the JSX lift and generic field join, the nine positive source-to-component rows were absent. `guarded` remains absent: the existing logical-AND lift carries the value operand and does not propagate the guard into the result. Rust and TS variants of :341 were ported; Kotlin was outside the repository's Rust/TS evaluation scope.

## 6. Read/write classification, report only

The source is the hardcoded Rust match in `/Users/chrishafley/projects/sprefa/v5/src/engine/decls.rs:1092`, `classify_call_kind(callee: &str) -> Option<&'static str>`.

| Bare callee input | Output |
|---|---|
| `execute`, `execute_batch`, `execute_returning` | `Some("write")` |
| `prepare`, `prepare_cached`, `query_row`, `query_map`, `query_and_then`, `query_named` | `Some("read")` |
| Other names | `None` |

It consumes the bare callee string. Receiver identity, SQL text, receiver type, and compiler resolution are not inputs. Classification is computed in `src/engine/extract/call.rs:108` and `:368`, then stored on `CallSiteBaseline.classification` at `:389`; storage interns it into nullable `_call_raw_site.classification_sid`.

`src/engine/family/call_kind.rs:30` scans `_call_raw_site`'s `caller_sid` and `classification_sid`, drops NULL classifications and `caller_sid == 0`, and emits distinct `(fn, kind)` pairs. A function can yield both a read and a write row. The SQL twin uses `SELECT DISTINCT caller_sid, classification_sid FROM _call_raw_site WHERE caller_sid != 0 AND classification_sid IS NOT NULL`.

The brief's `src/storage/call.rs:1057` and `:1125` are test fixture schema/projection sites. The name classification originates in `engine/decls.rs`; it is not loaded from a rule file.

Proposed placement in ryi: apply the same name table in a derived policy projection over the existing call `site.callee` and its containing DF `call_res`/`new` node. A nullable `call_kind` value would annotate that call-result row; unknown names remain NULL. The existing DF `function` is the owning function display column. To produce the v5 aggregate, select distinct owning definition identity plus `call_kind`, retaining both read and write. Use the enclosing CallF definition's blob/span as the identity so same-named functions remain separate. JSONL and SQLite would use the same `call_kind` column. This needs no type checking and no additional syntax walker. No classification implementation or schema change was made in this work.

## Gates, failures, and stops

Final targeted status: all 8 `v5_parity` tests pass without snapshot updates. The existing `t_13_flow_join` (2 tests) and `t_12_df_identity` (3 tests) passed after the replacement. The final source change only renamed the new containment helper; the 8-test parity gate was then rerun and passed.

The prescribed `cargo test --features cli --no-fail-fast` was run in `crates/sprefa-extract`, with `KACHE_DISABLED=1`. Each attempt stopped at 110 seconds. The final attempt stopped its process group. It did not finish the suite and is not a passing gate. Before that cap, the corrected test-registration rail and repository quality gate both passed.

Failures observed across the capped full-gate attempts include the TS7 rename/LSP rails, TS cleave/LSP and globals rails, the self-named initializer rail, resolver/flow fixtures without imports, revision/diff and capability checks, legacy kind-vocabulary / v5 golden parity rails, known witness failures, and an unavailable benchmark-oracle path. They are recorded individually in the error artifact. Apart from the baseline reproduction described below, these failures were not all classified against the base revision. No broad golden refresh, missing-tool install, or benchmark-corpus run was performed.

Exact compiler errors, failed-test text, and command-cap stops are retained in `0_ryi_v5_parity_errors.txt`. Early compilation failures were corrected before their item's commit. The stale HTTP schema hash was updated; the generated-contract check passed after staging regenerated code.

The old `t_167_graph_paths::flow_paths_follow_derived_interprocedural_edges` and `t_193_ts_rtkq_jsx::written_tsx_calls_are_additive_to_the_original_resolved_site` use a cross-file unimported `helper()`. Baseline `a44eced8` was compiled and queried on that reproduction: exit 0, only `call_site(run, helper, 33:41)`, no resolved edge. These failures were retained rather than changing fast resolver behavior.
