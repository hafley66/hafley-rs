Completed on `fix/e2e-llmock-green`, without pushing or installing boop. Baseline HEAD was `5247845c`; changes since the reported product base `2d666ded` were task briefs and planning/issue documents.

Every cargo test and boop process used sandboxed HOME. Baseline and green runs unset BOOP_DB and BOOP_MAIL_DIR at the test-process boundary. Fixture commands set their own store, readers, target root and tmux identity. Reclaim's unsafe default-socket baseline was intercepted by a temporary private-socket wrapper before running it. The committed fixture now owns a per-case socket and a scratch git repository. The contract runner retained its built-in readonly before/after favorite/tag count tripwire.

The integration files are modules of `--test main`, except OMP, which has its own target. Commands used `CARGO_BUILD_JOBS=4`, the prepared `_shared` cargo target, and `--test-threads=1`:

```sh
cargo test -p boop --test main tui_revive_e2e:: -- --test-threads=1
cargo test -p boop --test main worktree_reclaim_e2e:: -- --test-threads=1
cargo test -p boop --test main t1_harness_boundaries:: -- --test-threads=1
cargo test -p boop --test omp_live_trait_e2e -- --test-threads=1
```

First failures, before fixes:

| File | First failure | Evidence |
| --- | --- | --- |
| tui_revive_e2e.rs | Claude row remained `live` after `%0` was reused by a restored shell | `scratch/logs/baseline-revive.log:47` |
| worktree_reclaim_e2e.rs | Claude case 3: `both retired targets evicted` | `scratch/logs/baseline-reclaim.log:55` |
| omp_live_trait_e2e.rs | `real OMP transcript by UUID` | `scratch/logs/baseline-omp.log:20` |
| 1_harness_boundaries.rs | `FavoriteSourceKind::as_str` classified as dispatch | `scratch/logs/baseline-boundaries.log:28` |

Per-test causes and fixes. Paths below are relative to `crates/`.

| Failing test | Cause, file:line and class | Fix commit | Evidence line |
| --- | --- | --- | --- |
| `tui_revive_e2e::a_dead_coordinator_pane_revives_on_its_session_claude` | **Product:** `boop-proc/src/_2_gc.rs:608` accepted a reused pane as coordinator life. Require ownership of the bound conversation. | `bb3207b0` | `green-tui_revive_e2e-1.log:25`: `dead revive-e2e-claude … REVIVABLE`; pane `%0` runs sleep. |
| `tui_revive_e2e::a_dead_coordinator_pane_revives_on_its_session_codex` | **Product:** same reused-pane check. **Test:** `boop/tests/tui_revive_e2e.rs:657` expected the opening user prompt in Codex's resume viewport, which renders the persisted reply. | `bb3207b0`, `57c9880c` | `green-tui_revive_e2e-1.log:32`: dead/REVIVABLE; `fix-revive.log` captures the resumed reply and omitted prompt. |
| `tui_revive_e2e::a_dead_coordinator_pane_revives_on_its_session_opencode` | **Product:** same reused-pane check. | `bb3207b0` | `green-tui_revive_e2e-1.log:39`: `dead revive-e2e-opencode … REVIVABLE`. |
| `worktree_reclaim_e2e::worktree_reclaim_claude` | **Test:** `boop/tests/worktree_reclaim_e2e.rs:685` created fresh targets, protected by `boop-proc/src/_2_gc.rs:11`'s 24-hour retention. **Product:** disk admission selected path order rather than age at `_2_gc.rs:528`. **Test:** eviction events are on stderr. | `b2af8aaf`, `32c19bcb` | `green-worktree_reclaim_e2e-3.log:33`: retired-old removed before retired-new on the following line. |
| `worktree_reclaim_e2e::worktree_reclaim_codex` | **Test/product:** same target-age fixture, disk-admission ordering and stderr assumptions. | `b2af8aaf`, `32c19bcb` | `green-worktree_reclaim_e2e-3.log:51`: scratch db/socket and target ages; subsequent GC events remove old before new. |
| `worktree_reclaim_e2e::worktree_reclaim_opencode` | **Test/product:** same target-age fixture, disk-admission ordering and stderr assumptions. | `b2af8aaf`, `32c19bcb` | `green-worktree_reclaim_e2e-3.log:73`: scratch db/socket and target ages; subsequent GC events remove old before new. |
| `omp_live_panes_bind_distinct_sessions_and_project_real_transcripts` | **Product:** `boop-harness/src/harness/omp.rs:187` guessed a cwd directory by replacing slashes. Native transcripts existed in a canonical `--private-var-…-repo--` directory. Read transcript-header cwd across native layouts and canonical aliases. | `1920203f` | `diagnostic-omp.log` lists both existing native transcripts; `green-omp_live_trait_e2e-3.log:11` binds `%0`, UUID and transcript path, and line 12 binds a distinct `%1`/UUID. |
| `tmux_command_preserves_homes_and_serializes_fixture_overrides` | **Test cascade:** `boop/tests/omp_live_trait_e2e.rs:206` encountered the mutex poisoned by the preceding product panic. Serialization passes alone. | `1920203f` | `baseline-omp.log:26`: `PoisonError`; `isolated-serialization.log:10`: 1 passed. Full-file runs now pass both tests. |
| `t1_harness_boundaries::behavioral_harness_dispatch_stays_in_adapters` | **Test classification:** `boop/tests/1_harness_boundaries.rs:301`. `boop-store/src/1_user_slice.rs:26` serializes a closed source-kind enum, and `FavoriteSource::parse` parses legacy provenance. Their Codex names represent stored data; neither function selects adapters or invokes harness behavior. Restrict the exception to these two symbols. | `1e692c75` | `baseline-boundaries.log` reports five lexical matches in those symbols; `green-t1_harness_boundaries-3.log:23`: 2 passed. |
| `parentless_results_answer_the_latest_dispatch_sender_with_the_real_exit` | **Product:** `boop-proc/src/supervise.rs:2321` returned before writing a result when no parent existed. Resolve the latest dispatch sender, retaining a lane-local receipt if dispatch is absent. `boop/src/cli/job.rs:1540` preserves the dispatching caller. Parent edges stay absent. | `dbba9f17` | `parentless-before.log:14`: empty result list; expected rc 0, 7 and 129 on line 15. `lib-green.log:467`: 189 boop-proc tests passed. Contract case `contract.log:129` checks parentless wait exits 0 and 1. |

Additional fixture ownership changes: `40a86759` stamps revive subprocess paths, preserves actual TMUX/TMUX_PANE, excludes SQL headers from scalar results, and requires a positive stored assistant-turn count. `83d328b3` gives reclaim fixtures private tmux sockets, descendant tmux wrappers, sandboxed delete/query commands, and explicit coordinator reader paths. These address test/env ownership; the reproduced failures did not require a thread-local-root inheritance change.

The parentless contract case is committed separately in boop2-harmonize as `ca814e9`, at `/Users/chrishafley/projects/boop2-harmonize/tests/4_lane_wait.bats:4`. That checkout was detached; the commit remains local.

Evidence artifacts remain under ignored `scratch/logs/`. The report embeds the causal findings so the removed scratch stores are not needed to read it. `db-transcripts.ndjson:1` records the OpenCode coordinator's route, `%0`, UUID and matching `home/.local/share/opencode/opencode.db` cursor in the same scratch world. `capture-boop-reclaim-opencode-21311.txt:8` contains `FIXED_TERMINAL_REPLY`; line 22 contains the running-lane disk-low alarm. OMP's third-run transcript UUIDs are `01a0fd08-85f7-7000-9f84-ee1d2eedeb51` and `01a0fd08-8daa-7000-ba44-3424dae106ee`.

Supplemental evidence runs used a scratch Node HTTP relay forwarding exclusively to loopback llmock v0.1.2. Its request log is `scratch/logs/llmock-requests.ndjson`: 28 chat-completion POSTs, 20 Anthropic message POSTs, 18 Responses POSTs, seven Anthropic hello HEADs and one health GET. No record/upstream mode or remote provider was used. llmock v0.1.2's [router](https://raw.githubusercontent.com/larsakerlund/llmock/v0.1.2/src/main.rs) exposes provider endpoints and health, so the relay supplied the request capture. The three required green runs used llmock directly.

| Gate | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| tui_revive_e2e | 4 passed | 4 passed | 4 passed |
| worktree_reclaim_e2e | 3 passed | 3 passed | 3 passed |
| t1_harness_boundaries | 2 passed | 2 passed | 2 passed |
| omp_live_trait_e2e | 2 passed | 2 passed | 2 passed |

`cargo test -p boop-store -p boop-proc -p boop-harness --lib`: boop-store 247 passed / 1 ignored; boop-proc 189 passed; boop-harness 251 passed / 2 ignored. `cargo test -p boop --bin boop`: 170 passed. Formatting and `git diff --check` passed.

`cd /Users/chrishafley/projects/boop2-harmonize && BOOP_BIN=/Users/chrishafley/.cache/boop/lanes/_shared/debug/boop bash tests/run.sh`: exit 0, 83 TAP cases, **0 not-ok**, two existing v1 capability skips. The added parentless case passed.
Arguments:
  [COMMENT]  `comment_id` in `agent_turn_comment`. Required by the bare spawn spelling `boop beep
             fork <id>`; `join` and `diff` take their own
```

## ryi TS RTKQ and JSX fact goldens (2026-10-02)

Issue: `issues/ryi-ts-rtkq-jsx-golden/item.md`.
Branch: `feature/ryi-ts-rtkq-jsx`.

Scope follows the coordinator's final correction: extraction and existing
fact outputs only. RTKQ hook-to-operation derivation is excluded from this
increment. No analysis SQL files, graph analyses, CLI commands, or flags were
added. Component and prop resolution belongs to the separate slow lane; the
fast JSX rows here preserve written tag/attribute syntax and callable ownership.

### Changes and contracts

Copied the read-only dl5 `examples/openapi-sim/{openapi.json,components.tsx,hooks.ts}`
verbatim into `crates/sprefa-extract/tests/fixtures/rtkq_jsx/`. The new
`0_nested.tsx` covers nested JSX, member tags, fragments, boolean and spread
attributes, a nested declaration, a variable-bound arrow, member calls, and
nested calls. Comment/string lookalikes produce no expected syntax rows.

| Record | Payload columns |
| --- | --- |
| `call_site` | `callee`, `path`, `line`, `fn`, `start`, `end` |
| `jsx_element` | `name`, `path`, `line`, `fn`, `start`, `end`, `parent_start` |
| `jsx_attribute` | `path`, `element_start`, `name`, `value`, `start`, `end` |

`start` and `end` are UTF-8 byte offsets with exclusive end; `line` is 1-based.
Calls retain full written callee text, independently of resolution. `fn` names
the innermost callable, including variable-bound arrows; anonymous callables
use `<anonymous>`, file-level sites use `<root>`.

Element identity is `(path,start)`. `parent_start` is the enclosing JSX element's
start, or NULL at a JSX root. Fragments use `<fragment>`; member tags preserve
text such as `UI.Badge`. Attribute ownership is `(path,element_start)`.
Attribute values retain written syntax, including quotes/braces; boolean
attributes have NULL value. Spread attributes use name `..` and text such as
`{...props}`. These facts do not claim checker-resolved component symbols.

TypeSpec declares the three tables; generated DDL, catalog and typed writers
were regenerated. SQLite also supplies its standard `_row`, `_input_path`, and
`_content_id` export columns. These syntax payloads are emitted as project rows,
like the existing fast SCM rows, so their source coordinates are carried by
`path` and the payload offsets. The existing fast pass reuses the CST parse;
resolve retains captures while leaving the CST plane masked. Nested callees
retain query-match pairing, so `factory()()` has separate `factory` and
`factory()` callee texts.

### Golden fact rows

`tests/goldens/193_ts_syntax.jsonl` pins 25 rows: 11 `call_site`,
7 `jsx_element`, and 7 `jsx_attribute`. Paths are normalized to fixture basenames
only in the assertion. Payload offsets, lines, names, ownership and duplicates
remain pinned.

| components.tsx callee | line | enclosing fn | start | end |
| --- | ---: | --- | ---: | ---: |
| `useGetUserQuery` | 9 | `UserCard` | 460 | 479 |
| `useListUsersQuery` | 14 | `UserList` | 560 | 579 |
| `useCreateOrderMutation` | 19 | `NewOrderButton` | 673 | 697 |
| `useLazyGetUserQuery` | 24 | `PrefetchedUser` | 787 | 808 |
| `useDeleteWidgetMutation` | 31 | `WidgetRow` | 1060 | 1085 |

| 0_nested.tsx element | enclosing fn | start | end | parent_start |
| --- | --- | ---: | ---: | ---: |
| `article` | `Card` | 55 | 108 | NULL |
| `span` | `Card` | 78 | 98 | 55 |
| `section` | `Panel` | 175 | 317 | NULL |
| `Card` | `Panel` | 218 | 240 | 175 |
| `UI.Badge` | `Panel` | 245 | 267 | 175 |
| `<fragment>` | `Panel` | 272 | 304 | 175 |
| `footer` | `Panel` | 274 | 301 | 272 |

Attribute rows: article `title={title}`; section `id="panel"`, boolean `hidden`,
spread `{...props}`; Card `title={title}`; UI.Badge `count={1}`; footer
`data-label="end"`. Full positions and values are in the golden.

`tests/193_ts_rtkq_jsx.rs` asserts the same fact payloads across fast JSONL,
resolve JSONL, fast SQLite, resolve SQLite, and direct SCM projection.
`tests/fixtures/rtkq_jsx/1_dogfood.sh` is an unrun case script requiring an
already built binary through `RYII`; it compares existing output payloads and
reads the stored columns without deriving graph results. No frozen golden or
roster was changed; no dependency was added.

### Timing and verification status

Pre-change samples used the installed `/Users/chrishafley/.cargo/bin/ryii`:
`/usr/bin/time -p ryii fast /Users/chrishafley/projects/hafley-rxjs/packages`.
This is the packages corpus requested by `ryii fast packages`.

| sample | logging | real seconds | user seconds | sys seconds | JSONL rows |
| --- | --- | ---: | ---: | ---: | ---: |
| before 1 | default | 1.37 | 6.35 | 0.82 | 301603 |
| before 2 | `RUST_LOG=off` | 1.53 | 5.88 | 0.92 | 301603 |
| after | deferred by user stop instruction | unmeasured | unmeasured | unmeasured | unmeasured |

Before the stop instruction, an intermediate `cargo build --features cli --bin
ryii` completed through the installed rcargo wrapper (2m 04s). An intermediate
fast run emitted the five hook rows and the JSX rows shown above. That run
exposed the nested-callee pairing defect; the subsequent pairing fix has NOT
been compiled or executed. The golden corrects the inner call to `factory`
by source review; it has NOT been validated against the final implementation.
The rcargo wrapper overrides `CARGO_BUILD_JOBS` with `RCARGO_JOBS` (default 12);
the coordinator should set both to 4 for the deferred serialized gate.

A second build was terminated at the user's stop instruction. No cargo gate,
new Rust test, SQLite comparison, dogfood script, or after timing was run.
`git diff --check` reported no whitespace errors. The final source, golden and
script remain unverified at runtime.

Deferred serialized checks, to be run by the coordinator:

```sh
cd crates/sprefa-extract
CARGO_BUILD_JOBS=4 RCARGO_JOBS=4 cargo test --features cli
RYII=/absolute/path/to/built/ryii tests/fixtures/rtkq_jsx/1_dogfood.sh
RUST_LOG=off /usr/bin/time -p /absolute/path/to/built/ryii fast /Users/chrishafley/projects/hafley-rxjs/packages > /tmp/ryi-ts-after.jsonl
```

The crate gate has two declared pre-existing root-prefix oracle differences:
`golden_parity::ported_facets_match_v5` and `golden_parity::rust_doc_parity`.
They were not rerun or modified. No boop tests were run. No push was performed.

Commits before this report: `eb55297f` fixtures; `826a1a05` fact extraction and
storage; `9401921a` fact goldens and unrun dogfood script.
Arguments:
  [COMMENT]  `comment_id` in `agent_turn_comment`. Required by the bare spawn spelling `boop beep
             fork <id>`; `join` and `diff` take their own
```

## ryi TS exports lane, 2026-10-02

D2 before: plan reports 342/486 unresolved workspace import pairs. After: shared workspace package resolution implemented for edit plans and module fact bindings. Source rung order: exports types declaration map, tsconfig paths, package `dist` to `src` convention, then the declaration itself if no source exists. Direct source exports retain their target. Exact exports, wildcard exports, blocked exports, and one-source declaration maps are handled. No new commands, flags, analyses, or graph walkers.

D2 verification: UNRUN by coordinator instruction to serialize gates. `D02.sh` checks every workspace pair from SQLite specifier facts against both fast JSONL and SQLite resolved_import module rows, listing each missing pair before failure. `workspace_exports_resolve_to_sources_in_rung_order` checks map > paths > convention plus wildcard and blocked exports. Corpus is `/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-exports` at d0802620. No measured after count or corpus rung totals are available yet. Required release build, dogfood runner, and `cargo test --features cli` remain for the coordinator; use CARGO_BUILD_JOBS=4. Known pre-existing failures: golden_parity::ported_facets_match_v5 and golden_parity::rust_doc_parity.

D1 before: toSignal 2/22 files and GraphId 7/31 files, with silent success and new consumer type errors in the plan. After: fast inherits D2's workspace source resolver; slow retains TS7's edits and supplements exported root bindings using the existing importer binding walk across sibling workspace packages. Duplicate file/span edits are excluded. Property renames keep their compiler-selected receivers. UNRUN: `D01.sh` checks fast and slow committed file coverage (toSignal >=22, GraphId >=31), empty abstains, and zero new tsc diagnostics against a fresh pre-edit baseline in affected packages. No measured coverage or tsc result is claimed. The TS7 LSP and all build/test/dogfood gates await serialized coordinator execution.

D24 before: the two md declarations have no path-bearing records in fast JSONL (plan: 159 of 161 TS paths). After: fast JSONL emits existing file facts for `.d.ts`, `.d.mts`, and `.d.cts` inputs, including declarations with no symbols or occurrences. SQLite already emits file rows through its raw-input path. This is a fact-output fix. UNRUN: `D24.sh` asserts both `packages/md/src/0_css.d.ts` and `packages/md/src/style.css.d.ts` occur as file rows in fast JSONL and fast SQLite. Build, crate tests, and case execution remain for the coordinator.
## ryi TS edit-plan defects (2026-10-02, feature-ryi-ts-refactor)

Verification deferred by the user's stop instruction: release build and crate test processes were terminated; no completed gates. Case scripts below are unrun. The coordinator will run the release build, assigned dogfood cases, and `cargo test --features cli` serially. Existing golden-parity failures remain excluded from the no-new-failures requirement.

- D3 before: in-package moves could add the root workspace package as a dependency for an existing reference. After: dependency additions require changed package ownership. Case: `D03.sh`, unrun.
- D4 before: runtime imports were scanned, type-position imports were absent. After: the shared Oxc module scan emits type-only dynamic-import rows, including `typeof import`, for existing move rewrites. Case: `D04.sh`, unrun.
- D5 before: cleave removed the source's exported declaration and broke glob-barrel API. After: exported TS items retain a source re-export. Case: `D05.sh` includes fast/slow planning, unrun.
- D6 before: named `export { item, ... } from` statements were absent from caller edits. After: direct named re-exports are included and remain export statements when split. Case: `D06.sh` isolates named-barrel repair using isSignal; installMdviewHost also encounters D7's mutable-binding protection. Unrun.
- D7 before: host remained in ports.ts and the destination assigned an imported binding. After: cleave refuses a referenced mutable local binding that the drag plan leaves in the source, before writes, including shared host under `--drag`. This is conservative and also refuses read-only use of such a retained binding. Case: `D07.sh` covers both tiers, with/without drag, unrun.
- D8 before: generated imports were extensionless, always semicolon-terminated, and split each remaining specifier into its own line. After: relative module spellings follow the source suffix; new imports/re-exports follow quote and semicolon style; same-kind named bindings stay grouped. Existing caller statement style takes precedence. Case: `D08.sh`, unrun.
- D20 before: verification rollback restored files but left newly created destination directories. After: the journal records absent destination ancestors and removes them deepest first after restoration, retaining pre-existing directories and nonempty directories. Case: `D20.sh`, unrun.
- D22 before: dry-run accepted internal state roots that commit rejected. After: rename/move/cleave validate state location against the real target root before creating state, in both modes; symlinks are resolved through the nearest existing ancestor. Internal state is consistently refused because the commit engine requires external state. Case: `D22.sh`, unrun.
- ryi-move-stale-plan before: the reported repro replayed relocate edits after flags and source offsets changed. The current move runner rebuilds plans on each invocation. After: staged/commit state is additionally namespaced by a hash of every MoveArgs field, move-list bytes, canonical roots, and complete generated actions (including expected content hashes and replacement bytes). Case: `0_move_stale_plan.sh`, unrun; pass its full basename explicitly to run.sh.

Deferred case invocation (after the coordinator's serialized release build):

```sh
RYII="$CARGO_TARGET_DIR/release/ryii" CORPUS="$HOME/projects/rxjs-corpus-feature-ryi-ts-refactor" dogfood/ts/run.sh D03 D04 D05 D06 D07 D08 D20 D22 0_move_stale_plan
```

D03/D04/D05/D06/D08 additionally compare type diagnostics against fresh pre-edit baselines through `1_type_errors.sh` (default `node_modules/.bin/tsc`, overridable with `TSC`). D05 checks signals, signal-grid, and docs-kit. All checker calls are deferred. D06/D08 explicitly include their untracked probe files through the existing `--root` option. No analysis SQL, graph walkers, CLI commands, or flags were added.

Implementation commits: D3 `02f62a25`; D4 `5a740949`; D5/D6 `ce8a23be`; D7 `935acfb7`; D8 `6c16c9f6`; D20 `1fe6434d`; D22 `29a2c385`; stale request identity `20e3476b`. Later commits adjust unrun cases and D8 suffix typing. None has passed a build or runtime gate in this lane.
# ryi TS graph lane, 2026-10-02

Coordinator instruction suspended builds and all runtime gates. The initial
release build was terminated locally and remotely before completion. Cases below
are written but unrun; release build, dogfood and crate tests remain unverified.
Corpus: `~/projects/rxjs-corpus-feature-ryi-ts-graph`, detached at d0802620.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D9 | No callers for same-file arrow/function-expression consts | Lexical callable spans resolve g, k and nested inner; D09.sh checks those rows and corpus sectionsOf | Extraction/resolution facts |
| D10 | DOM globals and unrelated declarations bind through corpus_unique | TypeScript call-name fallback has no cross-file corpus-unique leg; plain calls require a lexical callable target or import/checker binding; D10.sh asserts globals, local forward const and parameter shadow | Resolution facts |
| D11 | invalidate binds SignalCreator to Query and enlarges the cycle | Local forward callable is extracted/resolved; phantom edge is absent; D11.sh asserts facts and existing stratify cycle output | Resolution facts |

D10 compatibility risk: previous name-only cross-file TypeScript resolutions
without imports are deliberately declined. Serialized gates must check legacy
fixtures that expected such resolutions. No frozen snapshots were regenerated.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D12 | 0_log → 0_0_log and 10_slice → 2_10_slice | Existing stratify proposals replace numeric/insertion prefixes before adding the depth prefix; D12.sh checks corpus and synthetic proposals | Existing command path proposal |
| D13 | Existing callers command emits 11 rows for 6 sites | Existing callers query chooses one row per full site/target span using SQLite row_number; closure/enclosing mirror facts remain available; D13.sh compares fast/slow rows to six fact sites | Existing callers output shape; facts retained |
| D16 | FILE#render and DiagramRenderCache.render silently return zero | FILE#name filters existing callers rows by canonical file path; Class.method returns an explicit usage error directing FILE#method (allowed by plan expected column); D16.sh asserts both | Existing callers anchor handling |

D16 does not derive class ownership. The callers relation carries callee path,
name and span. Class.method is explicitly rejected instead of reporting an empty
success. FILE#name retains all same-named declarations within the selected file.

| Defect | Before (plan evidence) | After (implementation, unverified) | Layer |
|---|---|---|---|
| D19 | Offset-only closure names remove/re-add unchanged call edges after earlier edits | Anonymous TS callers use named-owner + BLAKE3 closure bytes + duplicate-body ordinal; fast and SCIP slow share formatting; D19.sh asserts corpus delta and synthetic raw identity/diff invariance | Resolution fact identity, consumed by existing diff |

D19 identity scope: unrelated prefix or different-body sibling insertions preserve
identity. Edits inside the closure change its digest. Adding an identical closure
before another identical closure in the same named owner can shift ordinals.
Two existing TS closure-mirror expectations were updated for the declared identity
change; frozen extraction snapshots were not regenerated.

Static review: `git diff --check` and `bash -n` on D09, D10, D11, D12, D13,
D16 and D19 completed. No case bodies were executed. Build, crate test and
runtime correctness remain unverified. D13 selection prefers verified rows
and preserves the existing unresolved grade.

D10 lexical identifier coverage includes both calls and `new` expressions;
explicit local class constructors join their method declaration spans.
## ryi TS miscellaneous defects (2026-10-02)

The coordinator superseded the per-commit gate with a stop on builds and executions. All four release builds, dogfood cases, and `cargo test --features cli` gates are pending serialized verification. No cargo/rustc process was started in this lane.

- D18 before: mixed-grammar query aborted on `Invalid node type "call_expression"`. After: query compilation classifies `NodeType` errors per grammar, skips those files, and reports total and per-language skipped counts on stderr. Syntax and other query errors remain failures. `D18.sh` compares mixed-directory output with TS/TSX-only output and checks malformed-query rejection. Code and case are unverified.
- D21 before: `--resolve --sqlite` emitted zero occurrence rows. After: its existing raw-project path includes the same SCM symbol/occurrence/local lowering used by `fast`, reusing retained captures where available. Phase-two JSONL without SQLite is unchanged. `D21.sh` compares every occurrence field and multiplicity against `fast` on the same `**/*.ts` corpus; the plan's historical count was 53,684. This is a fact-output fix. Code and case are unverified.
- D23 before: advertised `dismantle` accepted arguments and returned a TODO. After: removed the operation from `ops.tsp`, generated args/CLI/client/daemon/server surfaces, dispatch, and captured root help. The permitted removal option was selected because deletion and cascade planning have no implementation. Generated surfaces were synchronized directly because the lane has no schema-local node_modules and installs are prohibited; generator parity is pending. The root help capture intentionally changes for this defect. `D23.sh` checks absence from help and clap rejection with exit 2. Code and case are unverified.
- D25 before: PATH text was compared directly with content-keyed flow endpoints, and bare git blob IDs did not name worktree content. After: the existing command hashes PATH bytes to the tagged content ID, accepts the exact `file.digest` / `flow_edge.from_blob` form, rejects reversed/empty/out-of-range PATH spans, and documents offsets, root/revision lookup, and digest semantics in help. No graph analysis, SQL analysis file, or walker was added. The plan's children parameter has local `df` flow; the existing command reads only interprocedural `flow_edge` rows. Consequently, nonempty paths for that example remain unresolved under the later restriction on graph analyses. `D25.sh` asserts the parameter/local-edge facts, PATH/digest equivalence, and retains the original nonempty-path assertion so this gap cannot pass silently. Code and case are unverified; D25 is partial, with a known expected-result gap. The graph help capture intentionally changes for this defect; wrapping parity requires the serialized help test.
# ryi TS slow lane, 2026-10-02

All code and dogfood cases are committed. Build, test, and runtime results are unverified. The user's later instruction stopped parallel builds/tests and deferred gates to the coordinator. No cargo, ryii, tsc, vitest, or dogfood execution was started in this lane; no installs or pushes were performed.

Corpus prepared outside the repository at `/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow`, detached at d0802620. The protected corpus directories were read only.

| Case | Before (plan measurement) | After (implemented, unverified) | Commits |
|---|---|---|---|
| D14 | Slow misses the 12 cache.render sites at lines 7,8,10,11,12,14,15,28,29,30,31,33. | Checker execution survives missing SCIP indexes. Targeted references open all supplied TS files and seed from extracted declarations. Existing `slow` and `--resolve --ts-checker` CLI outputs append tsgo destinations joined to raw call/type spans. D14.sh asserts all 12 checker-bound sites in graph, resolve, and slow SQLite facts. | 1fab89cb |
| D15 | Missing ts-checker can exit 0 with no slow edges, including an early no-index return. | Slow TS validates the feature before index discovery or targeted resolution, then exits non-zero naming ts-checker. D15.sh checks package/corpus roots and existing from/call-path/callers commands using a cli-only binary. Added missing-feature CLI regression test; corrected the case to the existing single-seed call-path syntax. | 71669f2f, a1d1da29 |
| D17 | Fast method rename exits 6 with 286 runtime-seat lines. | Typed method plans retain their existing behavior. Unresolved method receivers produce one Refused diagnostic naming --slow. Property declarations retain their existing dynamic-seat behavior. D17.sh asserts non-zero exit, one stderr line, no stdout plan, and unchanged source. | bc016456 |
| J01 | No tested slow JSX attribute-to-props declaration binding. | Existing resolved_edge binds the component. CST attribute identifier spans feed tsgo definition requests; symbol and occurrence def/ref rows bind a qualified FooProps.bar declaration. J01.sh asserts Foo, bar, exact UTF-8 source offsets, and the props member declaration in resolve and slow SQLite outputs. | 4718ffbb |

D14 changes facts and existing command behavior. No SQL analyses or graph walkers were added. No CLI commands, flags, or output variants were added.

J01 depends on the fast `jsx_element` and `jsx_attribute` record contract from `feature-ryi-ts-rtkq-jsx`, commit 826a1a05. That commit was not cherry-picked into this lane. Slow code reads existing CST facts and emits existing symbol/occurrence/resolved_edge variants; no fast JSX files were edited. A props reference uses a shared symbol of the form `tsgo <declaration-path>#FooProps.bar@<start>:<end>`; its source span covers the JSX attribute name, and the same symbol's def row identifies the member declaration.

The tsgo session first uses the crate's existing ts7 compiler installation, then a project/ancestor `node_modules/typescript/bin/tsc`. This worktree's ts7/node_modules is absent. No installation was attempted; the corpus links the already-installed project dependencies.

Coordinator gates still required, serialized with CARGO_BUILD_JOBS=4:

1. Merge the fast JSX record contract before J01.
2. In crates/sprefa-extract, run `cargo test --features cli`. Known pre-existing exceptions are `golden_parity::ported_facets_match_v5` and `golden_parity::rust_doc_parity`; no new-failure claim is made here.
3. Preserve the cli-only debug ryii path as RYII_NO_TS_CHECKER for D15.
4. Run `cargo build --release --bin ryii --features cli,ts-checker,typespec`.
5. Run `RYII=<release ryii> RYII_NO_TS_CHECKER=<cli-only ryii> CORPUS=/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow dogfood/ts/run.sh D14 D15 D17 J01`.

The scripts and Rust changes remain unrun. Runtime checker behavior, compilation, crate parity, and all four expected case outcomes require these gates.

## J01: slow JSX props member symbol fix (2026-10-02)

Failure: the coordinator's integrated J01 run failed
`assert.ok(refs[0].symbol.includes('#FooProps.bar@'))`. The JSX attribute
reference existed but its symbol named the enclosing props interface.

Cause: `crates/sprefa-extract/src/edit/1b_ts7_symbol_seed.rs:108` in
4718ffbb returned the first flat document symbol whose declaration range
contained the definition offset. The LSP session uses default client
capabilities, so tsgo supplies flat symbols. tsgo's
`internal/ls/symbols.go:65-90` flattens parents before children and uses
whole declaration ranges. `FooProps` therefore matched the offset of
`bar` before the `bar` row with `containerName: FooProps` was considered.
The replacement selection is at `1b_ts7_symbol_seed.rs:108-126`.

Change: select the smallest containing flat symbol range in the target
URI and retain its compiler-provided container name. The existing JSX fact
emitter at `crates/sprefa-extract/src/edit/1g_ts7_resolve.rs:208` receives
`FooProps.bar` and emits the shared def/ref symbol
`tsgo <declaration-path>#FooProps.bar@<start>:<end>`. Definition and reference
spans retain their existing byte coordinates. The regression test
`flat_symbols_select_props_member` covers parent-first and reversed order,
unrelated same-named members, foreign URIs, excluded end offsets, and a
Unicode prefix requiring UTF-16-to-byte conversion.

The cited utility plan contains no J01 row; its JSX coverage rows do not
contradict the fixture. `dogfood/ts/J01.sh` is unchanged.

Verification: `git diff --check` passed. No build, test, checker, install,
or dogfood script ran in this lane. Coordinator commands, run serially
from `crates/sprefa-extract`:

```sh
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo build --release --bin ryii --features cli,ts-checker,typespec
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo test --lib --features cli,ts-checker,typespec flat_symbols_select_props_member -- --test-threads=1
RYII=/Users/chrishafley/.cache/boop/lanes/_shared/release/ryii CORPUS=/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow STATE=/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow.J01-fix.state bash dogfood/ts/run.sh J01
## D12: stratify stem preservation and collision filtering (2026-10-02)

Failure: the coordinator observed 40 `stratify_move` records, including
`4_jsxAuto.test.ts -> 4_jsxAuto.ts` and repeated source paths for `0_log.ts`,
`2_Signal.ts`, `3_Endpoint.ts`, `8_sync.ts`, and `9_history.ts`.

Cause: before this change, `crates/sprefa-extract/src/0_stratify.rs:188`
emitted depth-prefix moves and `:242` independently emitted locality candidates
under the same `stratify_move` record. A locality candidate could point to a
neighbor with a different stem or to its own source. The existing `prefixed`
helper preserves the entire filename after `^[0-9]+[A-Za-z]*_`; it did not
truncate `.test`. Depth proposals also had no destination collision check.

Change: `crates/sprefa-extract/src/0_stratify.rs:174` collects one numbering
proposal per source; `:194` filters them before emission. The helper at `:594`
rejects every proposal sharing a destination and rejects occupied destinations
whose source will remain unmoved, including filesystem entries outside the
source set. Rejection repeats until dependent proposals are also removed.
Permutations of vacated source paths remain available. Identity proposals are
retained to satisfy D12.sh's three-row repro, including its already-numbered
`0_log.ts`. Collision filtering can leave reached strata without move proposals.
At `:247`, locality candidate reason, neighbor, cut lines and shared-edge count
are carried on the existing `locality` row. Only numbering proposals emit
`stratify_move`, and their full stems are preserved.

Coverage written: `tests/156_stratify.rs` asserts exact proposals for `.test`,
`.browser.test`, `.memory`, `.tsx`, insertion prefixes and identity proposals;
it rejects duplicate destinations, an unreached occupied source and an occupied
directory outside the file set, and compares repeated output. Existing locality
checks now read candidate metadata from `locality`. The source module test covers
cascading rejection and permutations. D12.sh is unchanged and agrees with the
plan row's prefix replacement rule.

Validation: `git diff --check` passes. No builds, tests, installs, node/npm or
dogfood scripts were run. Compilation and runtime behavior remain unverified.
The crate guidance's historical `v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md`
is absent in this checkout; the supplied D12 plan and case were read.

Coordinator commands, run individually from `crates/sprefa-extract`:

```sh
cargo test --features cli --bin ryii stratify::tests::collision_rejections_propagate_and_permutations_remain_available
cargo test --features cli --test all t_156_stratify
cargo build --release --bin ryii --features cli,ts-checker,typespec
RYII="${CARGO_TARGET_DIR:-$PWD/target}/release/ryii" CORPUS="${CORPUS:?set the coordinator corpus path}" dogfood/ts/run.sh D12
```

For the stronger corpus invariants, set RYII to that rebuilt binary's absolute
path and CORPUS to the coordinator corpus, then run:

```sh
(cd "$CORPUS" && "$RYII" stratify packages/signals/src --from packages/signals/src/index.ts) | jq -se '
  . as $facts |
  [.[] | select(.record == "stratify_move")] as $moves |
  ($moves | map(.from_path)) as $from |
  ($moves | map(.to_path)) as $to |
  ([$facts[] | select(.record == "stratum" or .record == "unreached") | .path] - $from) as $unmoved |
  ($moves | length) > 0 and
  ($from | length) == ($from | unique | length) and
  ($to | length) == ($to | unique | length) and
  ($to - $unmoved | length) == ($to | length) and
  all($moves[];
    (.from_path | split("/")[-1] | sub("^[0-9]+[A-Za-z]*_"; "")) as $stem |
    (.to_path | split("/")[-1] | sub("^[0-9]+_"; "")) == $stem
  )'
## D25 flow-path local df edges (2026-10-02)

Failure: `graph --flow-path packages/md/src/lib/1_tableModel.ts@3621:3640 packages/md`
returned empty stdout despite a `children` df parameter and outgoing local df
edges. The tagged BLAKE3 digest seed returned the same empty output. D25.sh
failed at `[ -s path.jsonl ]`; its expectation agrees with plan row D25.

Cause: `crates/sprefa-extract/src/0_graph.rs:75` previously obtained only the
resolved/TSI output, without a raw-fact sink. The insert filter also excluded
phase-one df rows. The flow arm in `plane_edges`
(`crates/sprefa-extract/src/0_graph.rs:270`) read only `flow_edge`, which holds
interprocedural edges. The local parameter's outgoing `edge` rows were absent
from the existing traversal's input.

Change: flow-path uses the existing raw+TSI resolve entry point, or the existing
raw slow entry point for --slow. Its sink retains file rows, df nodes and df
edges with `_input_path` and `_content_id`, then clears source metadata before
inserting resolved facts. The existing flow edge projection includes df edges
keyed by their stored content ID and endpoint spans. The existing
first-discovery traversal emits the existing graph_path records and witnesses
using the actual edge-table row IDs, including paths crossing local and
interprocedural edges. PATH/digest normalization is unchanged.

Added unrun regression coverage for a TS parameter's nonempty local paths,
byte-identical PATH/digest output, direct destinations and stored witnesses;
a unit fixture checks the complete three-hop local/interprocedural/local
record sequence and exclusion of call-family edges. D25.sh is unchanged.

Status: code complete, compilation and runtime behavior unverified. Only
static diff review and `git diff --check` were performed. No builds, tests,
installs, node processes or dogfood scripts were run.

Coordinator commands, run serially from `crates/sprefa-extract`:

```sh
CARGO_BUILD_JOBS=4 cargo test --features cli --bin ryii flow_paths_join_local_and_interprocedural_edges_with_stored_witnesses -- --test-threads=1
CARGO_BUILD_JOBS=4 cargo test --features cli --test all t_167_graph_paths:: -- --test-threads=1
CARGO_BUILD_JOBS=4 cargo build --release --bin ryii --features cli,ts-checker,typespec
RYII="${CARGO_TARGET_DIR:-$PWD/target}/release/ryii" CORPUS="$HOME/projects/rxjs-corpus-feature-ryi-ts-graph" dogfood/ts/run.sh D25
```
## ryi TS resolve regression repair (2026-10-02)

Code complete only. No cargo build/test/check, npm, node, installs, or dogfood
scripts were run. `git diff --check` passes. Runtime results remain pending
with the coordinator. All commands below run from `crates/sprefa-extract`,
using its single integration target `all`, one command at a time.

Repair commits:

- `011bfc60`: restore the corpus-unique call leg while retaining lexical callable
  targets, local value shadowing, import/receiver guards, and runtime-global exclusions.
- `62fd3c09`: preserve plain `.ts` resolve output and retain additive `.tsx`
  written syntax. A new test pins resolved `helper` at 33..39 beside written
  `helper()` at 33..41.
- `edbd2b74`: send D21 TypeScript SCM rows through the source/raw sink rather
  than adding them to the resolved return value. Rust raw output remains unchanged.

### Scope and deliberate fact changes

`TASKS/ryi-ts-rtkq-jsx-golden.BRIEF.md` row 2 explicitly requests call-site and
JSX facts in `fast` and `--resolve` **on .tsx**. Resolve syntax is therefore
additive for `.tsx`; plain `.ts` keeps its edge-only output. Fast continues
emitting `call_site`, `jsx_element`, and `jsx_attribute` wherever the queries
capture them. Resolved edges still use the OXC callee coordinates and their
existing serialization; written calls retain full-call coordinates.

The assertion changes in `tests/8_scip_families_cli.rs` implement that declared
brief row 2 addition: compare the legacy records byte-for-byte to the unchanged
`diet_scip_ts.jsonl`, separately allow the three new syntax record kinds, and
remove these additive fast-only rows when comparing plain `.ts` fast to resolve.
No golden was changed. `tests/193_ts_rtkq_jsx.rs` retains its existing syntax
payload golden across JSONL, SQLite, and direct SCM extraction.

Plan rows D10/D11 in `plans/2026-10-01-ryi-ts-utility.md` require DOM/lib globals
and lexically bound values to stay out of unrelated corpus edges. The repair
retains those guards rather than suppressing every unbound call. The finite
runtime-global spelling list is heuristic; it does not enumerate all host APIs.
D9 lexical callable facts and D19 closure identities remain present.

Plan row D21 requests occurrence parity in resolve SQLite output. These TS rows
now carry the source identity through `RawProjectFact`, while returned resolved
rows agree with JSONL. The new raw test compares all raw rows with direct
extraction and compares the resolved return value with `resolve_project`.

### t_1_resolve_cli::resolve_mode_streams_cross_file_edges

Cause: `7ed47bee` removed `call_name_match`'s corpus fallback and rejected every
plain call, including the fixture's unimported `helper`; current repair sites
are `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`.
`826a1a05` also appended plain-TS syntax at
`crates/hafley_scm/src/read/project.rs:269`.
Change: restore the guarded corpus leg and scope resolve syntax to TSX.
Gate: `cargo test --features cli --test all t_1_resolve_cli::resolve_mode_streams_cross_file_edges -- --exact --test-threads=1`

### t_1_resolve_cli::resolve_type_arm_streams_resolved_type_edges

Cause: `826a1a05`, `crates/hafley_scm/src/read/project.rs:269`, appended call
syntax even to a type-only resolve, because the input mask includes the call plane.
Change: plain `.ts` retains the original resolved-type stream.
Gate: `cargo test --features cli --test all t_1_resolve_cli::resolve_type_arm_streams_resolved_type_edges -- --exact --test-threads=1`

### t_1a_resolve_raw::raw_sink_gets_file_and_syntax_facts_from_the_resolve_inputs

Cause: `0d32142a`, `crates/hafley_scm/src/read/project.rs:329`, changed the raw
wrapper to append all SCM project rows to the resolved return value. Its Rust
fixture's returned rows consequently differed from `resolve_project`. Those
extra symbol rows came from the returned SCM section, not `flatten_each`.
Change: restore the resolved return contract; route D21 TS SCM rows through
`push_raw` with source identity and leave the Rust raw plane unchanged.
Gate: `cargo test --features cli --test all t_1a_resolve_raw::raw_sink_gets_file_and_syntax_facts_from_the_resolve_inputs -- --exact --test-threads=1`

### t_23_flow_cli_dispatch::call_and_flow_arms_emit_both_families

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed the call edge needed by the derived interprocedural flow join.
Change: restore `run -> helper` and its input to the existing flow join.
Gate: `cargo test --features cli --test all t_23_flow_cli_dispatch::call_and_flow_arms_emit_both_families -- --exact --test-threads=1`

### t_23_flow_cli_dispatch::flow_is_a_resolve_arm

Cause: `7ed47bee` removed the join's call edge at
`crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`; `826a1a05` additionally
introduced non-flow syntax rows at `crates/hafley_scm/src/read/project.rs:269`.
Change: restore the call input and retain the plain-TS flow-only stream.
Gate: `cargo test --features cli --test all t_23_flow_cli_dispatch::flow_is_a_resolve_arm -- --exact --test-threads=1`

### t_23_flow_cli_dispatch::resolve_without_family_is_byte_identical

Cause: `7ed47bee` removed the corpus-unique edge at
`crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`; `826a1a05` appended
plain-TS written calls at `crates/hafley_scm/src/read/project.rs:269`.
Change: restore the edge and retain the original edge-only default.
Gate: `cargo test --features cli --test all t_23_flow_cli_dispatch::resolve_without_family_is_byte_identical -- --exact --test-threads=1`

### t_4_capability_parity::every_library_capability_is_reachable_through_the_binary

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed the cross-file resolved call used as the ResolveCall CLI witness.
Change: restore the guarded corpus-unique leg through the existing CLI dispatch.
Gate: `cargo test --features cli --test all t_4_capability_parity::every_library_capability_is_reachable_through_the_binary -- --exact --test-threads=1`

### t_55_diff_verb::one_to_two_matches_the_hand_derived_rows

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed revision 2's unimported `beta -> alpha` edge from the diff fixture.
Change: restore the corpus edge at each revision; leave the diff code and golden unchanged.
Gate: `cargo test --features cli --test all t_55_diff_verb::one_to_two_matches_the_hand_derived_rows -- --exact --test-threads=1`

### t_55_diff_verb::a_dirty_worktree_does_not_change_the_delta

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed the same committed `beta -> alpha` edge; its expected delta was therefore absent.
Change: restore the committed-source corpus edge without changing revision reads.
Gate: `cargo test --features cli --test all t_55_diff_verb::a_dirty_worktree_does_not_change_the_delta -- --exact --test-threads=1`

### t_8_scip_families_cli::the_diet_scip_family_stream_is_the_fast_output

Cause: `826a1a05`, `crates/hafley_scm/src/read/lang/7_scm_rows.rs:147`, deliberately
adds written syntax to fast facts, while the test required the entire old stream.
Change: apply brief row 2's additive-record contract while keeping every legacy
row and its order pinned to the unchanged golden. The new syntax golden remains
covered by `t_193_ts_rtkq_jsx`.
Gate: `cargo test --features cli --test all t_8_scip_families_cli::the_diet_scip_family_stream_is_the_fast_output -- --exact --test-threads=1`

### t_98_resolve_witness::one_witness_per_leg_on_a_syntax_run

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed the fixture's resolved call, leaving no edge to witness.
Change: restore the edge through the existing witness serialization.
Gate: `cargo test --features cli --test all t_98_resolve_witness::one_witness_per_leg_on_a_syntax_run -- --exact --test-threads=1`

### t_98_resolve_witness::the_flag_off_stream_is_the_committed_golden

Cause: `7ed47bee` removed the edge at
`crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`; `826a1a05` appended
plain-TS written calls at `crates/hafley_scm/src/read/project.rs:269`.
Change: restore the edge and keep the flag-off plain-TS stream edge-only.
Gate: `cargo test --features cli --test all t_98_resolve_witness::the_flag_off_stream_is_the_committed_golden -- --exact --test-threads=1`

### t_167_graph_paths::flow_paths_follow_derived_interprocedural_edges

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed the resolve fixture's call edge, so its reference run had no flow edge.
Change: restore the call input to the existing flow derivation and graph path query.
Gate: `cargo test --features cli --test all t_167_graph_paths::flow_paths_follow_derived_interprocedural_edges -- --exact --test-threads=1`

### t_168_graph_revision::a_path_added_between_commits_is_reported_once

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771` and `:5194`,
removed revision 2's unimported `beta -> alpha` edge, yielding zero added paths.
Change: restore that corpus edge; revision path comparison code is unchanged.
Gate: `cargo test --features cli --test all t_168_graph_revision::a_path_added_between_commits_is_reported_once -- --exact --test-threads=1`

### t_golden_parity::call_resolve_scip_ratchet_ts

Cause: `7ed47bee`, `crates/hafley_scm/src/read/lang/ts.rs:4771`, removed the
public `call_name_match` corpus leg used by both resolution and the independent
ratchet twin, dropping corpus-unique true outcomes below the pinned floor.
Change: restore the heuristic with runtime-global exclusions. Keep the ratchet,
its floor, and `RATCHET.tsv` unchanged.
Gate: `cargo test --features cli --test all t_golden_parity::call_resolve_scip_ratchet_ts -- --exact --test-threads=1`

### Additional coordinator checks

These commands cover retained JSX facts, both new regression tests, the second
fast/resolve comparison, and D21's SQLite route:

- `cargo test --features cli --test all t_193_ts_rtkq_jsx:: -- --test-threads=1`
- `cargo test --features cli --test all t_1a_resolve_raw:: -- --test-threads=1`
- `cargo test --features cli --test all t_8_scip_families_cli::diet_scip_is_the_resolve_pass_with_both_arms_plus_the_scm_rows -- --exact --test-threads=1`

D10/D11 dogfood checks remain coordinator work after the serialized cargo gates.
The repair itself did not execute them.
## ryi TS cleave re-export regression (2026-10-02)

### t_156_cleave_play::the_demo_plays_four_cleaves_and_ends_green

Cause: commit `ce8a23bef8d414a1c72169b8a90800b0239a0a31` introduced an
unconditional source re-export for each exported TS declaration at
`crates/sprefa-extract/src/edit/_7_cleave.rs:1322` in that commit (current
condition at line 1344). Main's implementation contains no such insertion.
The demo's imports in `src/app.ts` and `src/report.ts` are direct named imports
that cleave repoints; no star barrel retains a route through `src/utils.ts`.
The inserted re-export also triggers the existing declaration-only fallback
at `crates/hafley_scm/src/read/lang/7_scm_rows.rs:809`: an export capture with
no contained definition marks root definitions exported at line 822. Thus
`tidy` is considered exported on the subsequent slug cleave, and
`crates/sprefa-extract/src/edit/_7_cleave.rs:2628` leaves it behind under
`--drag`. The fallback is present on main and was not changed here.

Change: the plan records whether another TS module has a star export that
resolves to SRC, using the existing module facts and TS resolver. Only that
retained barrel route enables the source re-export. Direct named imports
continue to move, and D6's named re-export splitting remains intact. D5's
`packages/signals/src/index.ts` contains `export * from "./2_Signal.js"`, so
D05.sh still requires and receives the source re-export; D06.sh still
repoints the moved named export and retains the remaining names. No
contradiction was found between these case contracts and the demo.
No extraction facts, existing assertions, goldens, dogfood scripts, SQL,
CLI commands, or flags were changed.

Coordinator filter, from `crates/sprefa-extract`:

```sh
cargo test --features cli --test all t_156_cleave_play::the_demo_plays_four_cleaves_and_ends_green -- --exact
```

### t_155_cleave_ts::source_exports_follow_retained_barrel_routes (new coverage)

The same cause is covered with two fixture variants: a star barrel retains
its source route and requires a source re-export; a named barrel moves an
aliased export and preserves its remaining export without adding a source
re-export. The test compares the complete source and barrel texts. Existing
`t_155_cleave_ts::commit_moves_the_item_and_its_imports` already covers direct
named import repair with an exact source-file assertion.

Coordinator filters, from `crates/sprefa-extract`:

```sh
cargo test --features cli --test all t_155_cleave_ts::source_exports_follow_retained_barrel_routes -- --exact
cargo test --features cli --test all t_155_cleave_ts::commit_moves_the_item_and_its_imports -- --exact
```

Verification: `git diff --check` passed. Compilation and runtime results are
unverified. No installs, builds, cargo tests/checks, npm, node, or dogfood
scripts ran. The coordinator must also run D05.sh and D06.sh through the
existing serialized dogfood runner.

## D2 workspace imports: code complete after workspace-source decision (2026-10-02)

Supersedes the stop recorded in `e165815a` on
`feature/ryi-ts-fix-workspace-imports`. The user classified `@hafley66/alloy-rs`
as a workspace package supplied by hafley-tsp at `8f679b1`. Its installed
copy's empty source maps are no longer the source route for D02. The supplied
failure remains `12/557 unresolved pairs`; no new result or count is claimed.

Resolver commit: `b56fdc64`. Dogfood commit: `c38fcb9b`.

`TsModuleIndex::build` discovers package identity from ancestor package.json
manifests of extracted inputs, including the separate TypeSpec input root.
It does not recursively scan the common ancestor of separate input roots.
The shared source resolver applies each matched package's export map,
declaration-map and tsconfig rungs, and dist-to-src fallback. Packages lacking
a src directory also try their package-root source stem. CSS uses runtime
conditions (`import`, `node`, `default`, `require`), then dist-to-src mapping.
Existing CSS files retain module facts when they are absent from extracted
inputs; those facts have source paths without invented blobs or definitions.

Every previously unresolved pair and its implemented route follows. Importers
and rxjs targets are relative to the rxjs corpus; TypeSpec targets are relative
to its sibling `<CORPUS>.tsp` checkout.

| Importer | Specifier | Change and target |
| --- | --- | --- |
| packages/json-rx/examples/4_cross_process_frame/1_generate.ts:4 | @hafley66/alloy-rs/adapters | Extra input root package name matches; exports and dist-to-src select `packages/rust/src/adapters/index.ts` in TypeSpec |
| packages/json-rx/examples/4_cross_process_frame/1_generate.ts:5 | @hafley66/alloy-rs/emitter | Extra input root package name matches; exports and dist-to-src select `packages/rust/src/emitter/index.ts` in TypeSpec |
| packages/grapht-golden/src/1_app.ts:15 | @hafley66/grapht-render-cytoscape | Package-root fallback selects `packages/grapht/adapters/2_render_cytoscape/index.ts` |
| packages/md/src/0b_SequenceDiagram.tsx:4 | @hafley66/grapht-render-cytoscape | Package-root fallback selects `packages/grapht/adapters/2_render_cytoscape/index.ts` |
| packages/md/src/2_MarkdownTable.tsx:6 | @hafley66/signal-grid/theme.css | Runtime export maps to `packages/signal-grid/src/theme.css`; module fact survives TS-only inputs |
| packages/md/src/plugins/1_FsTreeFence.tsx:3 | @hafley66/signal-grid/theme.css | Runtime export maps to `packages/signal-grid/src/theme.css`; module fact survives TS-only inputs |
| packages/md/src/plugins/1_FsTreeFence.tsx:4 | @hafley66/signal-grid/tree.css | Runtime export maps to `packages/signal-grid/src/tree.css`; module fact survives TS-only inputs |
| packages/md/src/plugins/1_MarblesFence.tsx:4 | @hafley66/signal-marbles/marbles.css | Runtime default selected over types; maps to `packages/signal-marbles/src/marbles.css`; module fact survives TS-only inputs |
| packages/boop-adapters/src/report-app/main.tsx:3 | @hafley66/report-shell/style.css | Runtime default selected over types; maps to `packages/report-shell/src/style.css`; module fact survives TS-only inputs |
| packages/boop-adapters/src/report-app/main.tsx:4 | @hafley66/report-shell/marbler.css | Runtime default selected over types; maps to `packages/report-shell/src/marbler.css`; module fact survives TS-only inputs |
| packages/vitest-telemetry/src/report-app/main.tsx:3 | @hafley66/report-shell/style.css | Runtime default selected over types; maps to `packages/report-shell/src/style.css`; module fact survives TS-only inputs |
| packages/vitest-telemetry/src/report-app/main.tsx:4 | @hafley66/report-shell/marbler.css | Runtime default selected over types; maps to `packages/report-shell/src/marbler.css`; module fact survives TS-only inputs |

`0_corpus.sh` creates the detached TypeSpec checkout beside the disposable
rxjs corpus, removes its write permissions, and checks its pinned HEAD and
clean status on reuse. It never resets or cleans that TypeSpec checkout.
D02 adds only `"$CORPUS.tsp/packages/rust"` to both input lists. Its SQL and
required-resolution assertions remain unchanged: every `@hafley66/` pair
must have both fast and SQLite module facts. No analysis, command, flag,
dependency, generated file, or exemption was added.

Added an unrun unit regression in ts_resolve.rs:
`workspace_inputs_resolve_root_sources_assets_and_extra_packages`. It checks
package-root TS, unconditional and conditional CSS, both TypeSpec subpaths,
blocked CSS exports, exclusion of an unsupplied neighbor, and exact module rows.
The fixture excludes CSS and manifests from its extracted corpus.

Only rustfmt and `git diff --check` ran. No build, cargo test/check, npm, node,
dogfood execution, install, push, or source worktree creation ran in this lane.
Code is complete; runtime verification is reserved for the coordinator.

Coordinator commands, run separately and sequentially from the repository root
using the prepared shared target directory:

```sh
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo test -p hafley_scm --features read --lib workspace_inputs_resolve_root_sources_assets_and_extra_packages
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo test --manifest-path crates/sprefa-extract/Cargo.toml --features cli --test 40_ts_resolve workspace_exports_resolve_to_sources_in_rung_order
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo build --manifest-path crates/sprefa-extract/Cargo.toml --release --bin ryii --features cli,ts-checker,typespec
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/lanes/_shared cargo test --manifest-path crates/sprefa-extract/Cargo.toml --features cli
```

Exact D02 command verifying all pairs above, from `crates/sprefa-extract`:

```sh
RYII=/Users/chrishafley/.cache/boop/lanes/_shared/release/ryii CORPUS=/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-workspace-imports-2 dogfood/ts/run.sh D02
```

The runner creates the sibling read-only checkout at
`/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-workspace-imports-2.tsp`.
The established golden_parity root-prefix failures remain coordinator context,
not results from this lane.
