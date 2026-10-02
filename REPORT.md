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
