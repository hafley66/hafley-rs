# Lane DV: `extract diff --from A --to B`

## Commits

| sha | subject | files |
| --- | --- | --- |
| c8733e42a7406fbc867d98d20ef3c3f5a8c2097e | feat(extract): diff verb snapshots two revisions through soopy and reads blobs from git | `src/4_watch.rs`, `src/5_diff.rs`, `src/bin/extract.rs`, `src/bin/extract/help.rs` |
| 2c70af8af2c78a1d45e65152504b7b15ae44a67b | feat(extract): diff verb resolves both ends and emits keyed fact deltas | `src/5_diff.rs`, `src/bin/extract/help.rs` |
| dacd2672002d321018435c15f00caa639ed62fbc | test(extract): diff verb fixture repo, expected rows, determinism and dirty-tree proofs | `tests/55_diff_verb.rs`, `tests/fixtures/diff/{make_repo.sh,expected_1_2.jsonl,expected_2_3.jsonl}` |

This report is the fourth commit of the lane, outside the three above: the file was already tracked at the base sha, so the rewrite is committed rather than left dirty in the worktree.

Not pushed. Each commit carries `Refs-Issue: @extract-diff-verb`.

| note | detail |
| --- | --- |
| `src/4_watch.rs`, one line, commit 1 | `default_patterns` became `pub(crate)` so the diff verb reuses the watch verb's pattern roster instead of carrying a second copy. `4_watch.rs:506`. |
| `default_patterns` reuse | `src/5_diff.rs:201` calls `crate::watch::default_patterns()`. |
| step-2 logic | copied, not shared: `diff_snapshots` (`4_watch.rs:293`) walks two `SourceSnapshot`s; the diff verb's `file_rows` (`src/5_diff.rs:494`) walks two `BTreeMap<path, blob_oid>` and adds the header count in the same branch. Same three branches, different input shape, so no helper was extracted. |
| step-4 call | `resolve_project` (`src/project.rs:236`) with `ResolveArms { call, types, flow: false }`, `ScipMode::Off`, no `project_root`: the fast path `extract fast --sqlite` runs. No resolver logic is new. |
| worktree hygiene | the commanded `CARGO_TARGET_DIR` leaves `crates/sprefa-extract/target-laned/` untracked and it is NOT gitignored; a plain `git add -A` would stage a build tree. I did not touch `.gitignore`, which is outside this lane's file list. |

## Keys

| relation | key as implemented | record tag | where |
| --- | --- | --- | --- |
| `file` | `path`; digest inequality is `change=changed` | `diff_file` | `src/5_diff.rs:494` |
| `resolved_edge` | `(caller_path, caller_name, callee_path, callee_name, kind)` + `resolution_origin` as the origin discriminator | `diff_edge` | `src/5_diff.rs:540` |
| `resolved_type_edge` | `(owner_path, owner_name, target_path, target_name, kind)` + `resolution_origin` as the origin discriminator | `diff_type_edge` | `src/5_diff.rs:569` |
| `resolved_import` | `(src_path, name, target_path, target_name)` | `diff_import` | `src/5_diff.rs:597` |
| `file_unresolved` | `(src_path, module, reason)` | `diff_unresolved`, `relation=file_unresolved` | `src/5_diff.rs:629` |
| `unresolved` (call sites) | `(path, detail, reason)` | `diff_unresolved`, `relation=unresolved` | `src/5_diff.rs:635` |

| property | implementation | where |
| --- | --- | --- |
| change words | `added`, `removed`, `origin_changed`; `changed` exists for `file` only | `src/5_diff.rs:240` |
| origin_changed | one base key, origin sets differ; paired lowest-first, the unpaired remainder is added/removed | `src/5_diff.rs:661` |
| set, not multiset | one row per (base key, origin); the representative span is the lowest `(start, end)` | `src/5_diff.rs:734` |
| span payload | B-side span when the row exists at B, else the A-side span; edges carry the call-site span, type edges the owner span, imports carry no span, `unresolved` the site span | `src/5_diff.rs:885` |
| sort | `(record rank, path, key)`; rank order is run, file, edge, type_edge, import, unresolved, so the file relation prints first | `src/5_diff.rs:395` |
| `--json` | accepted, no-op: JSONL is the only wire | `src/5_diff.rs:192` |
| `--sqlite` | new path only, private staging file, `persist_noclobber`; rows in six tables plus `_row` | `src/5_diff.rs:1004` |
| blob bytes | `soopy::GitBatch::read(&ObjectId)` per oid, cached across both revisions: the `cat_blob` shape of `0_query.rs:51` | `src/5_diff.rs:117` |
| step-4 corpus | each side's blobs are written into a `tempfile` scratch tree and resolved with the process rooted there, so every row's path stays repo-relative and no checkout is read | `src/5_diff.rs:111` |
| phase-1 reuse | unchanged blobs extract once: `dispatch`'s global blob cache (`cache::get_or_extract`, keyed by blake3 of the bytes) hits across the two resolves | measured below |

| measurement | value | how |
| --- | --- | --- |
| extractions, resolve at commit 1 | 4 of 4 blobs | throwaway test over `cache::EXTRACTIONS` |
| extractions, resolve at commit 2 | 2 of 4 blobs (a.ts, d.ts cached) | same |
| rows, commit 1 vs commit 2 facts | 1 vs 2 `resolved_edge` | same; test deleted after the measurement |

Caveat, stated because it bounds the claim: reuse rides the process-global cache capped by `SPREFA_EXTRACT_BLOB_CACHE_MB` (default 512 MiB). A corpus whose phase-1 output exceeds the cap re-extracts; the diff verb adds no cache of its own.

## Real corpus

Command, from a checkout of hafley-rs:

```
extract diff <checkout> --from 85e59e5d --to 20a76a34 \
  --pattern 'crates/sprefa-extract/tests/fixtures/kotlin_receivers/*.kt' \
  --pattern 'crates/sprefa-extract/tests/fixtures/kotlin/*.kt'
```

`diff_run` header, verbatim:

```json
{"record":"diff_run","from":"85e59e5d929864e8ba72eac992a6df2585271d04","to":"20a76a34143091441fb7c5a967fa5e3c091babbb","files_a":6,"files_b":8,"changed_blobs":0,"counts":{"file":{"added":2,"removed":0,"changed":0,"origin_changed":0},"resolved_edge":{"added":9,"removed":0,"changed":0,"origin_changed":0},"resolved_type_edge":{"added":1,"removed":0,"changed":0,"origin_changed":0},"resolved_import":{"added":0,"removed":0,"changed":0,"origin_changed":0},"file_unresolved":{"added":0,"removed":0,"changed":0,"origin_changed":0},"unresolved":{"added":1,"removed":0,"changed":0,"origin_changed":0}}}
```

| added `resolved_edge` by `to_origin` | count |
| --- | --- |
| receiver | 6 |
| module_plane | 2 |
| corpus_unique | 1 |

| other rows | count | note |
| --- | --- | --- |
| `diff_file` added | 2 | `kotlin_receivers/lib.kt`, `kotlin_receivers/use.kt`; the six `kotlin/*.kt` fixtures are byte-identical across the two shas, so `changed_blobs=0` |
| `diff_type_edge` added | 1 | `Holder.field: Widget`, `same_file` |
| `diff_unresolved` added | 1 | `use.kt` site `run`, reason `inferred` |

`receiver` edges appear exactly where lane K1 put them: `85e59e5d` predates the kotlin receiver legs, so all six receiver-leg call edges are additions. `git ls-tree` confirms the two `kotlin_receivers/*.kt` files exist only in `20a76a34`.

## Tests

| test | expectation | why |
| --- | --- | --- |
| `one_to_two_matches_the_hand_derived_rows` | stdout byte-equals `expected_1_2.jsonl` | the whole wire is pinned: header counts, both changed blobs with their oids, the edge added in `b.ts`, the edge removed and the edge added in `c.ts`, in sort order |
| `two_to_three_changes_one_blob_and_no_edge` | header `changed_blobs=1`, zero `diff_edge` rows, byte-equals `expected_2_3.jsonl` | a whitespace-only commit must move a digest and no fact; the brief's `changed_blobs=1` claim |
| `the_same_revision_is_the_header_alone` | exactly one line, `from==to`, `files_a==files_b==4`, every count zero | the empty-delta degenerate case, and that the header is not suppressed when there are no rows |
| `sqlite_refuses_an_existing_path_and_writes_nothing` | exit code 2, stderr names the existing file, the sentinel bytes survive, the directory listing is unchanged | the `--sqlite` law; the second half also catches a staging file left behind |
| `two_runs_are_byte_identical` | two runs of 1 to 2 compare equal | determinism: row order is a total order, nothing in the wire is machine-local (shas, oids, spans, repo-relative paths) |
| `a_dirty_worktree_does_not_change_the_delta` | after overwriting `b.ts` in the checkout and confirming `git status --porcelain` is non-empty, the output still equals the clean run and the golden | step 3's proof: bytes come from `git cat-file`, not the checkout |

| fixture commit | edit | hand-derived delta |
| --- | --- | --- |
| 1 `8ceebb11` | `a.ts` alpha, `b.ts` beta, `c.ts` gamma plus `useGamma` calling gamma, `d.ts` dvalue | baseline: one edge, `c.ts useGamma` to `c.ts gamma` |
| 2 `17518683` | `b.ts` beta calls alpha; `c.ts` renames gamma to delta and the call site follows | 2 changed blobs, edges +2 (`b.ts beta` to `a.ts alpha`, `c.ts useGamma` to `c.ts delta`), edges -1 (`c.ts useGamma` to `c.ts gamma`) |
| 3 `cf687b47` | whitespace only in `d.ts` | 1 changed blob, 0 fact rows |

| expected file | derivation | not copied from |
| --- | --- | --- |
| `expected_1_2.jsonl` | key values from the fixture source; spans counted from the file bytes (call sites `[42,47)` in `b.ts`, `[95,100)` in `c.ts`); kinds and origins read from the resolver vocabulary in `types.rs` and the existing `--resolve` goldens; shas and blob oids from the fixture's pinned commits | the diff verb's output |
| `expected_2_3.jsonl` | same; one `diff_file` row for `d.ts` | the diff verb's output |

The fixture script pins `GIT_CONFIG_NOSYSTEM=1`, an empty global config, fixed author/committer identity and a fixed date, so the three shas are identical on every machine and can be literals in the expected files.

| coverage gap | detail |
| --- | --- |
| `origin_changed` | implemented and counted, but no fixture commit produces one: no deterministic cheap edit flips a site's leg in the ts arm. Not asserted. |
| `file_unresolved` | the fast path emits no such row (`resolve_project` produces `ResolvedImportRow`, never `FileUnresolvedRow`; that relation belongs to `--deps`), so this relation is empty by construction in this verb. The arm is implemented and reachable if the fast path ever emits one. |
| `diff_import` | zero rows on the ts fixture, which imports nothing; the arm is covered by the type-checked match on `FlatFact::ResolvedImportRow`, not by an asserted row. |

## Gate

`cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`: rc 0, 181 suites, 963 passed, 0 failed. Last three non-empty lines:

```
   Doc-tests sprefa_extract
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The wall-clock ratio test `54_ts_module_plane::barrel_resolve_wall_grows_linearly_with_file_count` passed in this run; the subset run of `55_diff_verb`, `1_resolve_cli` and `54_ts_module_plane` was green in both runs.

## Blocked

Empty.
