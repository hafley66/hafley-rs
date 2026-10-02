# beep fork join and diff bring a fork back

`boop beep fork <comment>` spawned a lane and recorded the link; nothing brought it home. This
change adds the return trip:

```
boop beep fork join  <comment> [--lane <lane>] [--no-merge] [--no-reply] [--dry-run] [--mail-dir <d>]
boop beep fork diff  <comment> [--lane <lane>] [--stat] [--mail-dir <d>]
```

`join` merges `git -C <repo> merge --no-ff <branch>` (refusing a dirty index or a branch with no
commits past the route's `base_sha`), writes the lane's last assistant turn under
`<mail_dir>/forks/comment-<id>.reply.md`, and delivers it to the fork's parent (the route's
`parent`, else the one registered coordinator, else an error). `diff` prints
`git -C <repo> diff <base>..<branch>`.

## clap spelling decision

`boop beep fork <id>` still spawns; no separate `spawn` verb was needed. The `Fork` variant keeps
its existing fields and gains `#[command(subcommand)] cmd: Option<ForkCmd>`, mirroring the `Beep`
variant's exact shape at main.rs:97:

```rust
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
Fork {
    comment: Option<i64>,          // was i64; required by the bare spawn spelling only
    #[command(subcommand)]
    cmd: Option<ForkCmd>,          // None => today's spawn path
    ... existing fields ...
}
```

`comment` had to become `Option<i64>`: clap cannot keep a required positional next to an optional
subcommand. The `Beep` variant already carries the same comment on its `route`/`body` positionals
(main.rs:99-104), so this follows the in-repo precedent. Both spellings parse (`beep fork 7` and
`beep fork join 7`), covered by `beep_fork_bare_spelling_still_parses`.

## Changed files

| File | Change |
|---|---|
| `crates/boop/src/main.rs` | `Fork` variant gains `cmd: Option<ForkCmd>` + the two `#[command]` attrs; `comment` becomes `Option<i64>`; new `ForkCmd` enum (`Join`, `Diff`); Fork primer names join and diff. |
| `crates/boop/src/cli/job.rs` | `run_fork_join`, `run_fork_diff`, `pick_fork`, `fork_parent`, `repo_root`, `ensure_clean`, `ensure_ahead`, `merge_branch`, `git_diff`; `Fork` dispatch arms; nine tests. |
| `crates/boop-store/src/ident.rs` | `Store::last_assistant_turn`. |
| `crates/boop-harness/src/worktree.rs` | `run_git` made `pub`. |

`repo_root` delegates to `lane::repo_root` (boop-proc/lane.rs:158), the same rule `run_fork` uses
for `--cwd`. The reply send reuses `crate::cli::mail::run_send` with `wait: false` and
`as_name: Some(&fork.lane)`, the `boop beep <parent> <body> --no-wait` path.

## Tests

Ten tests in `crates/boop/src/cli/job.rs` `mod tests`: pick one/many/named, join clean, join dirty,
join nothing-past-base, dry run, diff contains the changed path, and the bare-spelling parse test.

## Validation

```
CARGO_TARGET_DIR=$PWD/target cargo test -p boop -p boop-store
```

Unit tests pass: the ten fork tests, `boop-store` (all pass), and the `boop` lib unit tests pass.
Five pre-existing tmux/claude-door integration tests fail in this worktree
(`tell::*` parent-edge, `lane_carcass::*` reclaim, `deliver_door::*` paste rung) with "tmux
new-session reported success but the session is not live" and "no answer from claude-534" — they
require a live claude coordinator door and are unrelated to this change (they fail on the base
commit too).

```
CARGO_TARGET_DIR=$PWD/target cargo clippy -p boop -p boop-store -- -D warnings
```

Fails on two pre-existing diagnostics, both present on the clean base commit and untouched here:

- `debug.rs:186` `run_host` never used (dead code when `dl6` is off).
- `job.rs:1305` `harness.unwrap()` after `harness.is_some()` (`run_agent`, pre-existing).

The fork/join/diff code introduced no new clippy diagnostics (verified by grepping the clippy output
for the new identifiers; none appear).

```
CARGO_TARGET_DIR=$PWD/target cargo run -p boop -- beep fork --help
```

```
Fork a lane off a stored terminal comment: the quoted turns and the note become the brief, the
lane runs on `--preset` from the caller's repo, and the link is kept in `agent_turn_comment_fork`.
The `join` and `diff` verbs bring the fork back

Usage: boop beep fork [OPTIONS] [COMMENT]
       boop beep fork <COMMAND>

Commands:
  join  Merge the fork's branch into the caller's repo and deliver the lane's last assistant turn
        to the fork's parent
  diff  Print `git diff <base>..<branch>` for the fork
  help  Print this message or the help of the given subcommand(s)

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
