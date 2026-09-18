# Lane DV: `extract diff --from A --to B`, the fact delta between two commits

Issue: `issues/extract-diff-verb/item.md` (`issuectl show extract-diff-verb`). Epic `extract-parity-move-rename`. Policy: `crates/sprefa-extract/AGENTS.md` "Division of labor" table, row "commit-to-commit delta": one shot, in this crate, soopy composition free; no daemon, no persistent cross-run index.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. 16 GB machine.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha except `54_ts_module_plane::barrel_resolve_wall_grows_linearly_with_file_count`, a wall-clock ratio test that flakes under a parallel gate and passes alone.
- Read, in order: `crates/sprefa-extract/AGENTS.md`; `src/4_watch.rs` whole (the soopy seam: `Options` `:121`, `run` `:135`, `diff_snapshots` `:293-325`, the delta match `:225-260`, `emit_replacement`, `parse` `:451`); `src/0_query.rs:43-60` (`source_bytes`, `cat_blob`: bytes of a git object without a checkout); `src/bin/extract.rs:570-600` (how `watch` and `query` are dispatched by argv[1]); `src/schema.rs:48-56` (the `resolved_edge`, `resolved_type_edge`, `resolved_import`, `file_unresolved`, `file` record shapes); `src/project.rs` `resolve_project_inputs` and the `fast` path that `extract fast --sqlite` runs; `crates/soopy/src/_0_types.rs:239-260` (`Revision::{Worktree, Named, Commit}`, `RevisionId`) and `:559-580` (`SourceSnapshot`, `SourceDelta`).
- Build: `cargo build --features cli --bin extract`.

## Files you own

- `crates/sprefa-extract/src/5_diff.rs` (new; the verb, mirrors `4_watch.rs`'s shape)
- `crates/sprefa-extract/src/bin/extract.rs` (dispatch line for `diff` beside `watch`, and the help text block that lists verbs)
- `crates/sprefa-extract/src/bin/extract/help.rs` (one entry for `diff`)
- `crates/sprefa-extract/tests/55_diff_verb.rs` (new)
- `crates/sprefa-extract/tests/fixtures/diff/` (new)

Not yours: `src/4_watch.rs` (read it; if you need a helper from it, make it `pub(crate)` in a one-line commit and say so in REPORT.md), `src/project.rs`, `src/types.rs`, `src/schema.rs`, anything under `src/lang/`, `crates/soopy/**`.

## The verb

```
extract diff ROOT --from <rev> --to <rev> [--pattern GLOB]... [--family call,type] [--sqlite PATH] [--json]
```

`ROOT` is a git repository root (or a linked worktree of one). `<rev>` is anything `git rev-parse` accepts; resolve both to full shas up front and print them in the header. Steps:

```
step 0  snapshot A = soopy SourceTree snapshot at Revision::Commit(sha_a), patterns applied   -> files_a: path -> blob id
step 1  snapshot B = same at Revision::Commit(sha_b)                                          -> files_b
step 2  deltas = diff_snapshots(files_a, files_b)  (reuse or copy the 4_watch.rs:293 logic)  -> Added / Removed / Changed(before, after)
step 3  bytes for a blob come from `git cat-file blob <oid>` (the 0_query.rs:51 shape), never from the working tree
step 4  resolve at A over ALL of files_a, resolve at B over ALL of files_b (the fast path, in memory, same code `extract fast` runs); phase 1 rows for a blob unchanged between A and B are computed once (blob-keyed cache; phase 1 is a pure function of the bytes)
step 5  set-difference per relation, keyed WITHOUT byte spans
```

Keys for step 5:

| relation | key |
| --- | --- |
| `file` | path; `digest` inequality is the changed set (this is step 2 restated, print it first) |
| `resolved_edge` | (caller_path, caller_name, callee_path, callee_name, kind, resolution_origin) |
| `resolved_type_edge` | (owner_path, owner_name, target_path, target_name, kind) |
| `resolved_import` | (src_path, name, target_path, target_name) |
| `file_unresolved` | (src_path, module, reason) |
| `unresolved` (call sites) | (path, callee detail, reason) |

Output, JSONL on stdout, one record per row, `record` = `diff_file` / `diff_edge` / `diff_type_edge` / `diff_import` / `diff_unresolved`, each with `change` = `added` / `removed` / `origin_changed` (same key except `resolution_origin`; carry `from_origin` and `to_origin`), plus a header record `diff_run` with `from`, `to`, `files_a`, `files_b`, `changed_blobs`, and per-relation counts. `--sqlite PATH` writes the same rows to a new database (refuse an existing file, the `--sqlite` law in `extract --help`). `--json` is a no-op alias for the default (JSONL) so the flag exists; document that.

Spans: every diff row may carry the B-side span when the row exists at B, the A-side span when it exists only at A; spans are never part of the key.

Determinism: rows sorted by (record, path, key). Two runs on the same shas produce byte-identical stdout.

## Verification, in order

1. Fixture `tests/fixtures/diff/`: a script `make_repo.sh` that builds a throwaway git repo in a temp dir with three commits over a 4-file ts corpus (`a.ts`, `b.ts`, `c.ts`, `d.ts`): commit 1 baseline; commit 2 adds a call `b.ts -> a.ts#foo` and renames `c.ts#bar` to `baz` (so one edge added, one removed, one file changed digest); commit 3 changes only whitespace in `d.ts` (digest changes, zero edge diffs). Hand-write the expected `diff_*` rows for (1 -> 2) and (2 -> 3) in `tests/fixtures/diff/expected_1_2.jsonl` and `expected_2_3.jsonl`.
2. `tests/55_diff_verb.rs`, through the real `extract` binary: (a) `1 -> 2` byte-equals `expected_1_2.jsonl`; (b) `2 -> 3` header shows `changed_blobs=1` and zero `diff_edge` rows; (c) `A -> A` is the header plus nothing; (d) `--sqlite` on an existing path exits nonzero and writes nothing; (e) two runs of (a) are byte-identical; (f) the working tree is dirty during the run (touch a file) and the output does not change, proving step 3 reads git objects, not the checkout.
3. Real corpus: `extract diff <repo root> --from 85e59e5d --to 20a76a34 --pattern 'crates/sprefa-extract/tests/fixtures/kotlin_receivers/*.kt' --pattern 'crates/sprefa-extract/tests/fixtures/kotlin/*.kt'` from a checkout of hafley-rs; the header and the per-origin added counts go in REPORT.md (expect `receiver` edges added, since 85e59e5d predates lane K1).
4. `cargo test --features cli --test 55_diff_verb --test 1_resolve_cli --test 54_ts_module_plane`.
5. Full gate.

## Commits

Subjects exactly:

- `feat(extract): diff verb snapshots two revisions through soopy and reads blobs from git`
- `feat(extract): diff verb resolves both ends and emits keyed fact deltas`
- `test(extract): diff verb fixture repo, expected rows, determinism and dirty-tree proofs`

Trailer on each: `Refs-Issue: @extract-diff-verb`. Do not push.

## Deliverable

`REPORT.md` at the worktree root. Overwrite the stale one there. Tables only. Sections:

1. `## Commits` : sha, subject, files.
2. `## Keys` : the key per relation as implemented, with the file:line.
3. `## Real corpus` : the step-3 header and per-origin counts.
4. `## Tests` : test, expectation, why.
5. `## Gate` : last 3 lines.
6. `## Blocked` : empty, or the exact error and the diff you wanted.

## Laws

- Stop and write `## Blocked` when a command fails in a way this brief did not anticipate.
- Do not end your turn until REPORT.md is written and the commits exist.
- No em dashes. No praise. Facts and receipts.
- Rust comments: at most 2 consecutive comment lines; state only constraints the code cannot show.
- Tests are integration tests through the real `extract` binary against a real git repo built by the fixture script. No mocks, no fakes. Expected rows are hand-derived from the fixture commits, never copied from the binary's output.
- No new resolver logic: resolve at A and at B is the existing fast path called twice. No watcher, no daemon, no persisted index between runs.
