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

## scm++ relational predicates, burndown row 9

Code complete on `feature/scmpp-relational`. No cargo build, check, or test was run.
No installs, dependency changes, ast-grep imports, vendored code, or pushes were
performed. Scoped rustfmt and `git diff --check` completed. Coordinator gates are
pending; the inline expected match lists have been authored without execution.

The entry-point seam in `crates/hafley_scm/src/lib.rs` now invokes the predicate
stage's query preparation before compiling the outer query. The rest of the
implementation is in the assigned types, walks, predicate pipeline, match
pipeline, tests, and relations docs. Other lanes' language readers are untouched.

| Relation | Syntax, with negated form available | Semantics | CSS equivalent | Test row names |
| --- | --- | --- | --- | --- |
| has | `(#has? @c kind+ [neighbor\|end])` or `(#has? @c (pattern) [options])` | Strict descendants; direct children with neighbor; preorder with end; named and anonymous nodes | `A:has(B)` / `A:has(> B)` | `has_pattern`, `has_neighbor`, `has_field`, `has_stop_inclusive`, `has_pattern_neighbor_miss`, `has_stop_before`, `not_has_pattern`, `has_left`, `has_right` |
| has-ancestor | `(#has-ancestor? @c kind+ [neighbor\|end])` or `(#has-ancestor? @c (pattern) [options])` | Strict ancestors, nearest first; direct parent with neighbor | `B A` / `B > A` | `ancestor_pattern`, `ancestor_neighbor`, `ancestor_neighbor_miss`, `ancestor_field`, `ancestor_stop_before`, `ancestor_stop_inclusive`, `not_ancestor_pattern`, `ancestor_value` |
| has-parent | `(#has-parent? @c kind+)` or `(#has-parent? @c (pattern) [options])` | Direct parent for all stop modes | `B > A` | `parent_pattern`, `parent_neighbor`, `parent_end`, `parent_end_strict`, `parent_field`, `parent_stop_inclusive`, `not_parent_pattern`, `parent_value` |
| precedes | `(#precedes? @c kind+ [neighbor\|end])` or `(#precedes? @c (pattern) [options])` | Later named siblings; adjacent with neighbor; search to sibling-list end with end | `A:has(+ B)` / `A:has(~ B)` | `precedes_neighbor`, `precedes_end`, `not_precedes_neighbor`, `not_precedes_end`, `precedes_pattern_neighbor`, `precedes_pattern_end`, `not_precedes_pattern`, `precedes_stop`, `precedes_stop_inclusive`, `precedes_right`, `not_precedes_right` |
| follows | `(#follows? @c kind+ [neighbor\|end])` or `(#follows? @c (pattern) [options])` | Earlier named siblings; adjacent with neighbor; search to sibling-list beginning with end | `B + A` / `B ~ A` | `follows_neighbor`, `follows_end`, `not_follows_neighbor`, `not_follows_end`, `follows_pattern_neighbor`, `follows_pattern_end`, `not_follows_pattern`, `follows_stop`, `follows_stop_inclusive`, `follows_left`, `not_follows_left` |
| nth-child | `(#nth-child? @c N [of kind\|(pattern)])` | Positive 1-based index among named siblings, optionally filtered before counting; captured node must pass filter; root has no index | `A:nth-child(N)` / `A:nth-child(N of B)` | `nth_child`, `nth_child_of`, `not_nth_child`, `not_nth_child_of`, `nth_child_pattern`, `not_nth_child_pattern`, `named_nth`; root case in `relational_query_syntax_and_root_capture` |

Options on directional relations: `stopBy: neighbor`, `stopBy: end`,
`stopBy: (pattern)`, and `field: name`. Legacy trailing stop words remain valid.
Pattern stops are inclusive and end the walk at the first matching visited node,
including the whole descendant preorder walk. Field tests refer to the related
node's own position in its parent. Default reach remains end. Parent reach stays
direct. Duplicate stop/field options, malformed positions, unknown grammar kinds,
and unknown grammar fields produce errors.

Tree-sitter `Query::new` determines argument boundaries through parser error
positions and acceptance of candidate replacements, then compiles each related
pattern against the language grammar. The preparation step keeps no nesting
counter or s-expression parser. Comments, native predicates, alternation, and
root captures have dedicated syntax rows. Related patterns are anchored to the
visited node by a reserved root capture and a cursor start-depth limit.

Positive relations bind captures from the first matching related node in walk
order. Bindings are appended to the candidate's capture list only after predicate
acceptance, mapped to global capture-name IDs, and exported to arena spans and
emissions. Negated relations export no bindings. Extension predicates consuming
related captures must follow the binding predicate. Native outer-query text
predicates execute before relation evaluation; native related-text filters belong
inside the nested pattern. Stop patterns do not export bindings.

All tests reside in `crates/hafley_scm/tests/_0_relational_predicates.rs`, using
Rust and TypeScript fixtures and literal inline snapshots with `assert_eq!`.
Relation rows carry CSS comments for the later servo oracle lane. Bounded-walk
comments apply to their fixtures; arbitrary stop patterns have no single general
CSS combinator translation. No servo oracle has been implemented.

Coordinator commands, run serially:

```sh
CARGO_BUILD_JOBS=4 cargo test -p hafley_scm --test _0_relational_predicates relational_
CARGO_BUILD_JOBS=4 cargo test -p hafley_scm --test neovim_predicates
CARGO_BUILD_JOBS=4 cargo test -p hafley_scm --test 0_emit
```

The first filter covers every relation. Narrow filters are `relational_siblings`
for precedes/follows, `relational_nth_child` for positions,
`relational_patterns_and_options` for has/ancestor/parent and pattern options,
`relational_fields_and_anonymous_siblings` for fields,
`relational_bindings_emit_and_filter` for exported captures,
`relational_argument_errors` for validation, and
`relational_query_syntax_and_root_capture` for native query syntax.

The updated user documentation is
`crates/sprefa-extract/docs/2_scm-with-ast-grep-relations-20260920.md`.

Final relation-specific edge rows and coordinator filters:

| Relation | Additional row names / case | Narrow cargo test filter |
| --- | --- | --- |
| has | `strict_descendant`, `direct_capture`, `negated_binding`, rejected binding leaves arena rows/spans empty | `relational_has_strict_and_rejected_bindings` |
| has-ancestor | `nearest`, `bounded`, `negated_no_bindings`, with nested Rust/TS functions | `relational_ancestor_nearest_and_bounded` |
| has-parent | Reusing outer capture name on related root retains both nodes | `relational_parent_reused_capture` |
| precedes | `adjacent_comment`, `past_comment`, `not_adjacent_comment` | `relational_precedes_named_comment_boundary` |
| follows | `adjacent_comment`, `past_comment`, `not_adjacent_comment` | `relational_follows_named_comment_boundary` |
| nth-child | `comment_counts`, `of_excludes_comment`, `not_comment_counts` | `relational_nth_child_named_comment` |

Use each filter with `CARGO_BUILD_JOBS=4 cargo test -p hafley_scm --test
_0_relational_predicates FILTER`. The earlier `relational_` filter includes all
these cases. Relation-specific test changes were committed separately.
