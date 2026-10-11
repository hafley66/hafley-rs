## Round 1

Critic B. Read in full: the plan, `schema/cli/domain.tsp`, `schema/cli/ops.tsp`, every file under
`crates/ryi-proto/src/gen/` and `crates/ryi/src/gen/`, `crates/ryi-proto/src/1_help.rs`,
`1a_verb_help.rs`, `crates/sprefa-extract/src/bin/ryi.rs`, and the handlers it dispatches to:
`bin/ryi/{0_cli,0_fast_receivers,0_revision,1_inputs,ops,0b_deadline,1c_no_inputs}.rs`,
`0_graph.rs` + `0a_graph_target.rs` + `0c_graph_walk.rs`, `5_diff.rs`, `4_watch.rs`, `0_stratify.rs`,
`0_query.rs`, `3_region_writer.rs`, `edit/_6_move.rs`, `edit/_5_move_text.rs`, `edit/_6_rename.rs`.
`edit/_7_cleave.rs`: lines 1-1100 read, 1100-3190 grepped for `cli`/`slow`/`root` only (planning internals).

### 1. The comparison flag: `--rev A..B`, one flag

What the code does today (`0_graph.rs:850-868`):
`before = ask_at(--at)`, `after = ask_at(--compare)`, then `changed_paths((at_sha, before), (compare_sha, after))`
tags `removed` with the `--at` sha and `added` with the `--compare` sha. So `--at` is the **base** and
`--compare` is the **target**. diff (`5_diff.rs:48-49`): `--from` base, `--to` target. Same orientation.

The tsp help on graph `compare` says "Diff the answers against REV" (`ops.tsp`, `ops_auto.rs:163`), which reads
as compare = base. That help text already has the direction backwards relative to the code. Two flags with
no direction in their names produced that drift once; the plan's `Revision { rev, compare }` keeps the same
two names.

Proposal: `Revision { /** REV, or BASE..TARGET */ @valueName("REV|BASE..TARGET") rev?: string }`.
- Orientation is git's `git diff A..B`: A base, B target. Matches both handlers with zero reorder.
- One field: one model member, one (value_name, action, help) tuple for the rail. No `@requires("rev")`
  on a second flag, no constraint that has to be spread.
- Parse: `split_once("..")`. `git check-ref-format` forbids `..` inside a ref name, so the split has one
  reading. Reject `...` (merge-base) with exit 2 rather than silently taking it as `A` + `.B`.
- graph: `--rev A` = answer at A (today's `--at`); `--rev A..B` = path changes (today's `--at A --compare B`).
  The existing "requires --call-path, --type-path or --flow-path" check (`0_graph.rs:853`) stays as the guard.
- diff: `--rev A..B` required range; `--rev A` alone is exit 2 "diff takes BASE..TARGET"
  (or `A..HEAD` if the user wants git's empty-side default; `RevisionReader::with_revision` takes any
  commit-ish so `HEAD` works).
- Wire: `diff_run` rows keep `from`/`to`, `GraphPathChange` keeps `revision`. Output field names stay.

Against `--rev A --compare B`: the reader has to know which side is base; the plan's own help string got it
wrong. Against keeping `--from/--to` on diff: `--from` also means "graph seed" and "stratify entry", which
is the collision the plan exists to remove.

Neither side reads the worktree: `0_revision.rs:41-43` errors "is not a commit" on anything else. So
"compare HEAD to my uncommitted tree" is not expressible under any spelling today; `A..` meaning
A..worktree would be new handler work, out of this plan's scope.

### 2. `--slow` has closer to one meaning than the plan's table says

Plan: "graph: SCIP edges; cleave/rename: compiler plan". Code:
- `graph --slow --callers/--uses` without `--scip-index` -> `0a_graph_target.rs::facts`: TypeScript LSP
  (`ts7_callers`, `ts7_graph_target`) + rust-analyzer `target_calls`/`target_types`. Compiler tier.
- `graph --slow --from/--call-path` on a Rust seed -> `0c_graph_walk.rs`: rust-analyzer `demand_walk`. Compiler tier.
- `graph --slow` otherwise -> `slow_project(paths, root, None, true)` = SCIP with checkers on (`checkers: true`).
- `rename --slow`: TS7 checker / rust-analyzer. `cleave --slow`: `SlowTsFacts` + `ts7_cleave_diagnostics` for TS.
- `ryi slow` (the verb): SCIP + checkers (`run_slow`, `checkers = !no_checker`).

So `--slow` = "the slow tier, compiler-backed" in all 3 ops plus the `slow` verb. The stale part is graph's
help string ("Walk the SCIP oracle's edges (ryi slow)"). Renaming graph's flag to `--oracle scip` or
`--scip-index` would split one tier across two spellings. Suggest: keep `--slow` on all 3, fix the help to
one string, and the rail keeps it as a single tuple.

### 3. `--sqlite` on graph is a different meaning

`Store.sqlite` doc: "Write to a new SQLite database instead of stdout". graph's is "Keep the fact store in a
new SQLite database at PATH" and graph still prints answers to stdout (`0_graph.rs:78-81, 886`). Spreading
`Store` into graph either lies in graph's help or the rail flags it forever. Either graph keeps its own spelling
(`--keep-store PATH`?) or Store's doc becomes neutral ("New SQLite database at PATH") and each verb's
stdout behavior is documented in the verb's about text.

### 4. Rail tuple `(value_name, action, help)` will never go green as written

Help strings that differ for real reasons today:
| spelling | ops | why they differ |
|---|---|---|
| `--list` | cleave, move, rename | 3 TSV row formats |
| `--json` | cleave, rename | plan JSON vs abstains JSON |
| `--text-refs` | cleave, move, rename | SRC / old-path / old-name |
| `--timeout` | graph (default 30), query (requires scmpp) | default and requires differ |
| `--state` | cleave/move/rename vs region | "outside the corpus" vs "for --apply" |
| `--kinds` | root (`KINDS`, includes cfg) vs watch (no value_name, no cfg) | watch mask lacks cfg (`4_watch.rs:533-548`) |
| `--arms` | root (call,type,flow; requires resolve) vs diff (call,type) | diff `parse_arms` rejects flow |
| `--root` | Inputs "Corpus root" vs watch/diff "Repository root (default: git root of cwd)" vs cleave "(default: git root of SRC)" vs rename "(default: git root of the first anchor)" | default differs per verb |

Suggest the rail key on (value_name, action, value type) only, and a separate help-text check that is per-model
(a model-spread flag cannot drift because it has one doc). Or the plan needs a row for every table entry above.

Counted from `ops_auto.rs` + `file_args.rs` + `inputs.rs` (python over `#[arg(long...)]`): 113 long flags,
66 distinct spellings, 23 spellings declared in more than one struct (plan says 24):
sqlite 8, root 6, lines 4, scip-index 4, state 4, fast-receivers 3, scip-timeout 3, from 3, slow 3, list 3,
commit 3, text-refs 3, pattern 3, scip-build 2, timeout 2, at 2, rust-checker 2, ts-checker 2,
go-checker 2, verify 2, json 2, kinds 2, arms 2.

### 5. `Inputs` spread into rename/cleave/move/diff: concrete blockers

- **stdin over the daemon.** `daemon_auto.rs:49-68` `request_uses_stdin` lists `paths` only for verbs whose op
  has `@bodyRoot input: RawByteStream`; `client_auto.rs:187` streams stdin only for
  extract/fast/slow/scip/graph/query/region/ingest. The plan's new op sigs for rename/cleave/move/diff
  have no `@bodyRoot`, so `ryi rename ... -` would hang or read nothing through the daemon. stratify already
  has this gap today (`"stratify" => &[]` with `...Inputs`). Add `@bodyRoot input: RawByteStream` to every op
  that spreads Inputs, stratify included.
- **`--root` presence is load-bearing in the edit verbs.** `RenameCx::open_with_untracked(&root, cli.root.is_some())`
  (`_6_rename.rs:183`), `MoveCx::open_with_untracked(&root, cli.root.is_some())` (`_7_cleave.rs` build and
  run_list). Passing `--root` switches untracked files into the corpus. `Inputs.root` keeps the same
  `Option`, so this survives, but the plan's "PATH given = occurrences only in those files" must say what
  untracked inclusion does when PATH is given and `--root` is not.
- **move's `root: path[]`.** `_6_move.rs:24-29` multi-root mode: one plan per root, rows assigned by
  prefix, `--verify-cwd` defaults to first root, rollback across roots. "Extra roots become PATH inputs"
  changes PATH from "corpus filter" (Inputs doc) to "additional plan root". That is a second meaning for
  PATH. Options: keep `--root` repeatable everywhere (Inputs.root becomes `path[]`, every other verb rejects
  >1), or move gets `--roots`.
- **`--entry`/`--depth` at a revision.** graph under `--at` builds `selected` from `inputs.paths` only and
  uses `watch::default_patterns()`, ignoring `--pattern` and `--entry` (`0_graph.rs:822-850`). diff passes
  `selected: None` (`5_diff.rs:88`). Spreading Inputs into diff exposes `--entry/--depth/PATH` that the
  revision path does not read. Either the handlers honor them (RevisionReader already takes `selected`;
  `reach_files` would need the scratch tree) or the rail/handler rejects them under `--rev`.
- **`graph --at` requires `--root`; diff defaults to git root of cwd** (`5_diff.rs:139`). One `--rev`
  should carry one root rule. diff's default is the one that needs no extra typing.
- **`Edit` model on move** adds `--json` (move has no JSON plan output) and puts `--list` behind one doc
  although the 3 row formats differ. Edit as written is cleave's flag set; move also has `--verify-cwd`.

### 6. Verbs the plan does not touch

- **watch**: `root`, `pattern` (with its own help and default-patterns list), `kinds`, `receipts`. It is the
  third hand copy of `--root`/`--pattern` after diff. Same `Inputs` question; watch's corpus is
  `soopy::SourceQuery { patterns }`, not `inputs::expand`, so PATH/entry would be ignored there too.
- **region**: `--apply` vs `--commit` on the other 3 edit verbs, both "write instead of dry run". `--state`
  is shared with the edit verbs.
- **query `--digest OID`**: a blob selector, the per-file cousin of `--rev`. No change proposed, noting it.
- **rename `--verify-scip INDEX`** vs `--scip-index FILE` elsewhere: same thing (a SCIP index path),
  two spellings, two value names.
- **stratify `--from PATH[:NAME][=RANK]`** vs graph `--from [PATH#]NAME`: both are "seed of a reach walk".
  The collision is the anchor separator (`:` vs `#`), and stratify's `:` also collides with graph `--slice PATH:BYTE`.
  Alternative to `--rank`: keep `--from` on both with `#` as the one anchor separator (`PATH[#NAME][=RANK]`),
  the same `#` rename (`FILE#OLD`) and cleave (`SRC#ITEM`) use.

### 7. Text that names flags and is not a clap arg (rail cannot see it)

- `ops.tsp` graph `flow_path` @doc: "--at reads the revision".
- `ops.tsp` rename `@afterHelp`: "3 ambiguous (pass --at)", "8 the --slow engine refused".
- `_6_rename.rs:32` comment, `:511-513` error "--at cannot be combined with --list".
- `1_help.rs:14,16` TOP: `ryi diff --from <REV> --to <REV>`, `ryi stratify --from <ENTRY>`.
- `1a_verb_help.rs:13,15` examples: `ryi diff --from HEAD~1 --to HEAD`, `ryi stratify --from src/main.ts`.
- `0_stratify.rs:277` error "pass at least one --from".
- `0_graph.rs:854` error "--compare requires ...".
- `1a_verb_help.rs:46` hardcodes `"from" => "reachable set"` keyed by arg id.
Test files naming the flags (`grep -rl` over `crates/sprefa-extract/tests crates/ryi*`):
`"--at"` 9, `"--from"` 9, `"--to"` 2, `"--compare"` 1, `"--slow"` 22.

### 8. Wire note

`Request.args` is `serde_json::to_value(args)` of the generated structs (`client_auto.rs:130-151`), decoded
by the same structs in `server_auto.rs`. Field renames (`at`->`rev`, `from`->..., move `root` Vec->Option)
change the daemon JSON. Handshake stamp (`executable_stamp`) restarts a daemon whose binary changed, so a
rebuilt `ryi`+`ryii` pair is fine; a stale `ryii` on PATH with a new `ryi` is not covered.

### Order suggestion

Step 1 of the plan (failing rail) needs the rail's key decided first (section 4), else the snapshot of
"today's collisions" is mostly help-text noise from `--list`/`--text-refs`/`--json`.

## Round 2

Read `review-a.md` in full. Spot-checked A's citations; they hold except where noted.

### Agree (A and B land on the same answer independently)

| topic | A | B | shared evidence |
|---|---|---|---|
| comparison flag | `--rev A..B` | `--rev A..B` | `0_graph.rs:850,856,743-766`; `5_diff.rs:48-49`; ops.tsp `compare` help reversed |
| `--slow` | one tier, keep on 3 verbs | same | `0_graph.rs:112-119`, `0a_graph_target.rs`, `0c_graph_walk.rs:22-23` |
| stratify | keep `--from`, `#` separator | same | `0_stratify.rs:292` (`rsplit_once(':')`) vs `0_graph.rs:369` (`split_once('#')`) |
| move `root: path[]` | PATH gets 2 meanings | same | `_6_move.rs:24-29, 228-286` |
| `open_with_untracked(.., cli.root.is_some())` | yes | yes | `_6_rename.rs:183`, `_7_cleave.rs:172, 738` (confirmed both lines) |
| Revision x Inputs | unspecified, refuse unread fields with exit 2 | same | `0_graph.rs:697, 826-848`; `5_diff.rs:88` |
| hand text outside the rail | `1_help.rs`, `1a_verb_help.rs` | same, plus error strings | |

Additional evidence for the orientation, which A and B both read off the handler: the only test,
`tests/168_graph_revision.rs:28-44`, runs `--at shas[0] --compare shas[1]` and asserts
`change == "added"` with `revision == shas[1]`. The test pins "--at is base, --compare is target".
`tests/fixtures/ryi_help/graph.txt:26` and `tests/snapshots/all__t_178_ryi_help__help.snap:90` freeze
the reversed help string ("Diff the answers against REV"), so the drift is snapshotted too.

### A found, B missed (these change or extend round 1)

1. **`Edit.list` carries `@conflictsWith(#["target","dest"])`** (ops.tsp cleave). Those arg ids are
   cleave's positionals; move has `old/new` and rename has `target/new`. A shared model cannot carry
   that conflict. clap panics in debug builds on an unknown id in `conflicts_with` (debug_asserts),
   so spreading as written fails at `help_command()` construction, i.e. every test, before the rail
   runs. B round-1 section 5 said Edit adds `--json` to move; A's point is the harder blocker. Options:
   one positional pair across the three ops (e.g. `target`, `dest`), or the conflict stays per op.
   This also applies to the plan's `Checkers` model if it carried any per-op ids; as written it only
   says `@requires("root")`, which resolves in every op that spreads `Inputs`.
2. **Duplicate parsers** `parse_mask` (`ryi.rs:998` / `4_watch.rs:533`) and `parse_arms`
   (`ryi.rs:979` / `5_diff.rs:149`). B listed `--kinds`/`--arms` as help drift; A shows two
   implementations of each concern, which CLAUDE.md "One implementation per concern" covers. A shared
   `Kinds`/`Arms` model needs one parser each, plus a decision on the two defaults
   (`FamilyMask::DEFAULT` vs `ALL`; `call` vs `call+type`).
3. **stratify `--kind call|type|both`** is a third spelling of resolve arms. B missed it.
4. **`--rev A..B` keeps `rev` on the request and `from_*/to_*` on output.** A's casing note. Agree:
   output fields describe the sides and stay as they are (`diff_run.from/to`, SQLite `from_sha/to_sha`,
   `5_diff.rs:1110-1113`). The plan should say so explicitly so a writer does not "fix" the wire.
5. **`RenameRequest.at`** (`_6_rename.rs:612`) and the doc comments. Count correction: `grep -- "--at"`
   over `rust_rename.rs`, `ts_rename.rs`, `prolog_rename.rs` returns 9 lines (A says 7 places).
   `ts_rename.rs:94` holds two in one line, so the occurrence count is 10.
6. **`--list` conflicts are clap-enforced only on cleave**; move (`_6_move.rs:507`) and rename
   (`_6_rename.rs:518`) check at runtime. A shared Edit model would move all three to clap, which changes
   the exit path (clap usage error, exit 2) but not the code. Neutral, noting it.
7. **stdin `-` has two meanings**: path list (`1_inputs.rs:20`) vs content bytes (ingest
   `ryi.rs:1161`, region `--generated`). `request_uses_stdin` treats both the same. B round 1 flagged only
   the missing `@bodyRoot` on rename/cleave/move/diff/stratify; this is a separate collision on the `-` value.

### Disagree

1. **Fallback `--rev A --rev B`.** A lists it as the fallback if the emitter grows `@maxItems`. Against:
   - Direction encoded by argv order is the same defect as `--rev A --compare B`: nothing in the
     spelling says which side is base. `A..B` puts the base on the left in the token itself.
   - It turns `Revision.rev` into `string[]` for every verb, including graph's single-revision case,
     and every handler then checks `len() <= 2`.
   - It needs an emitter feature (`@maxItems`, none in the schema today: only `@minItems(1)` at
     ops.tsp:345); `A..B` needs none.
   I'd drop it from the plan rather than carry it as a fallback.
2. **`A..` meaning A..worktree "without adding a third flag".** A and B both note `RevisionReader`
   refuses non-commits (`0_revision.rs:41-43`). B would keep that out of the plan entirely. Accepting
   `A..` with an empty right side should be exit 2 now, not reserved syntax, so the help does not
   advertise a form the handler rejects.
3. **A section 3: "settle whether the per-language checker booleans survive next to `--slow`".** It is a
   fair question, but out of this plan's stated scope (one spelling, one meaning). `--rust-checker`
   etc. have one meaning each ("add this checker's evidence to the fast tier"); `--slow` means "run the
   slow tier". graph already encodes the relation: `--slow` conflicts with all three
   (`ops_auto.rs:153`) and `load_store` zeroes them under `--slow` (`0_graph.rs:73-75`). Different
   granularity, consistent. Leave as is, but the `Checkers` model should carry the
   `conflictsWith slow` only on ops that have `--slow`, which again means the constraint stays op-level.

### Round-1 positions after A

- `--rev A..B`: unchanged, now with the test pin above.
- `--slow`: unchanged.
- Rail key: unchanged (key on value_name/action/type; help checked per model). A did not address the rail
  key; the drift table A built (section 6) is the evidence that help-text equality fails today.
- New blocker added to section 5: shared models cannot carry per-op arg ids in `conflictsWith`/`requires`.
  Rule for the plan: every constraint inside a shared model names only fields of that same model or of
  `Inputs`. Everything else is op-level.

## Round 3

Re-read `review-a.md` in full, including A's Round 2. One new read for this round:
`edit/_1_move_cx.rs:77-300` and `edit/_1_rename_cx.rs:53-75` (corpus walks).

### Correction to A's Round 2 (and to B round 1): untracked behavior on move

A's Round 2 says move "never" adds untracked files because it calls plain `MoveCx::open`
(`_6_move.rs:293`). In the code it is the reverse:
- `MoveCx::open` calls `walk_files(root)` (`_1_move_cx.rs:80-85`).
- `walk_files` is `walk_files_with_untracked(root, true)` (`_1_move_cx.rs:258-260`).

So move **always** takes the full walk.

The two modes of `walk_files_with_untracked` (`_1_move_cx.rs:262-300`):
- `include_untracked = false`: `git ls-files -z`, i.e. tracked files only.
- `include_untracked = true`: a `WalkBuilder` with `.ignore(false)`, `.git_ignore(false)`,
  `.git_global(false)` and `.git_exclude(false)`. This includes **gitignored** files too, minus
  `SKIP_DIRS = [.git, target, node_modules, .boop-worktrees]` (`:27`).

The corpus rule today, per verb family:

| verbs | corpus without `--root` | corpus with `--root` | source |
|---|---|---|---|
| move | full walk incl. untracked + gitignored | same | `_6_move.rs:293`, `_1_move_cx.rs:80, 258` |
| rename | `git ls-files` (tracked) | full walk incl. gitignored | `_6_rename.rs:183`, `_1_rename_cx.rs:62` |
| cleave | `git ls-files` (tracked) | full walk incl. gitignored | `_7_cleave.rs:172, 738` |
| Inputs verbs (fast, slow, scip, graph, query, stratify, root extract) | `ignore` walk: respects .gitignore, includes untracked non-ignored, roster extensions only | same (`--root` is not a corpus switch there) | `1_inputs.rs:116-181` |

That is 3 corpus rules and 1 flag (`--root`) that switches between 2 of them. `RenameCx::open`
(`_1_rename_cx.rs:56-58`) defaults to `true` as well, so only the CLI path sees the tracked-only mode.

### Final positions on the open points

1. **diff `--rev A` alone.** Exit 2: "diff takes BASE..TARGET". `A..` with an empty side also exits 2
   now. I take A's point that `A..` must not mean `A..HEAD`, because a later worktree meaning would
   silently change scripts. The help shows `REV|BASE..TARGET` only. The `A..` = A-to-worktree form
   waits for `RevisionReader` to accept a worktree (`0_revision.rs:41-43`). Converged with A.

2. **What `-` means.**
   - The `PATH` positional from `Inputs` is a path list (`1_inputs.rs:20-43`).
   - ingest `paths` and region `--generated` are content bytes (`ryi.rs:1161`, `3_region_writer.rs:83`).

   These are different arg ids on different ops, and neither ingest nor region spreads `Inputs`, so
   the rail never compares them. Leave both as they are and state each meaning in that arg's own
   help. Same as A.

   The plan has to add `@bodyRoot input: RawByteStream` to every op that spreads `Inputs`:
   stratify today, plus rename, cleave, move and diff after the spread. A rail row then enforces
   "spreads Inputs ⇒ `request_uses_stdin` lists `paths`" (`daemon_auto.rs:49-68`). A's correction
   holds: the daemon child has `Stdio::null()` (`client_auto.rs:39`). So `-` without a body expands
   to nothing (`1_inputs.rs:26-30`). It does not hang. Today `ryi stratify -` already does this.

3. **move `--roots` vs a list-valued `root`.** Use `--roots DIR` (repeatable) on move, with move also
   spreading `Inputs` for its single `--root`. `roots` gets `@conflictsWith(#["root"])` at op level,
   which is legal under rule 5, since both are fields of move's own op.
   - With one `--roots` value, move behaves exactly like `--root`. `plan_of` already branches on
     `len()` (`_6_move.rs:24-29`).
   - This keeps `Inputs.root` as `path?` everywhere, so no runtime `>1` checks and no allowlist row.
   - I withdraw my round-1 option "Inputs.root becomes `path[]`", for A's reason: there is no
     `@maxItems` in the emitter.

   Converged with A, who leaned `--roots`.

4. **Untracked side effect of `--root`.** Given the table above, I'd decouple corpus membership from
   `--root` presence: `--root` names the root, and nothing else.
   - All three edit verbs take one rule. The full walk is what move already does on every run, and
     what rename and cleave do whenever `--root` is passed. It is also the only mode `RenameCx::open`
     and `MoveCx::open` expose by default.
   - The tracked-only `git ls-files` mode is reachable today only by omitting `--root` on rename or
     cleave. If the user wants it kept, it becomes an explicit flag in `Edit`
     (e.g. `--tracked`, doc "Corpus is `git ls-files` only").
   - Which way to go is the user's call. The plan has to name one, because after the spread
     `Inputs.root` has one doc.
   - Two notes for the plan:
     - The edit verbs' full walk includes gitignored files. The `Inputs` walk excludes them. So
       "PATH given = occurrences only in those files" (plan, "Handler change") filters through
       `inputs::files`, which drops gitignored files that the edit corpus includes. That is a
       fourth rule unless the handler intersects the two lists explicitly.
     - The plan names `inputs::files(&Inputs)`. The function is `inputs::expand` (`1_inputs.rs:17`).

5. **Edit model constraint rule.** A `@requires` or `@conflictsWith` inside a shared model names only
   that model's fields or `Inputs` fields. Every other constraint lives on the op. Applied:
   - `Edit.list`'s conflict with the positionals moves to each op: cleave `target, dest`,
     move `old, new`, rename `target, new`. The alternative is one positional pair across the
     three ops.
   - `Checkers` keeps `@requires("root")`, which is legal because `root` comes from `Inputs`.
     Graph's `conflictsWith slow` (`ops_auto.rs:153`) stays on graph.
   - `Revision.rev` drops `@requires("root")` (A Round 2 §4: one root rule, diff's
     `git_root_of_cwd`). Graph's `conflictsWith [checkers, scip_index, sqlite]` stays on graph.
   - `Edit` should not add `--json` to move or `--verify` to rename. The fields that are in all
     three ops today are `list`, `state`, `commit` and `text_refs`. `verify` (cleave and move) and
     `json` (cleave and rename) stay op-local.
   - I think clap's debug asserts reject an unknown id in `conflicts_with`, but this round was
     read-only with no cargo, so that is unconfirmed. The rule holds either way.

### Agreed by A and B

- `--rev REV|BASE..TARGET`, one field, base on the left, `...` rejected. diff `--rev A` and `A..` are exit 2.
  Output keeps `from_*`, `to_*` and `revision`. Pinned by `tests/168_graph_revision.rs:28-44`.
- One root rule for revisions: diff's `git_root_of_cwd`, and graph drops `@requires("root")`.
- `--slow` stays on graph, rename and cleave as one tier. Graph's help gets fixed.
- stratify keeps `--from`, with `#` as the one anchor separator. `--rank` is dropped.
- graph's `--sqlite` is a meaning collision. `Store` gets the neutral doc "New SQLite database at PATH".
  Each verb's about text states whether stdout still carries rows.
- The rail keys on `(value_name, action, value type)`. Help text is checked once per shared model.
  Op-local spellings that are shared get one allowlist row each. The `--root` value_name drift
  (`DIR` vs the implicit `ROOT`) is a real catch.
- Every op that spreads `Inputs` gets `@bodyRoot`, with a rail row to enforce it.
- Under `--rev`, input fields the handler does not read exit 2 instead of being dropped silently.
- move multi-root becomes `--roots`.
- `parse_mask` and `parse_arms` each collapse to one parser in the same change as any shared
  `Kinds`/`Arms` model. Their defaults (`DEFAULT` vs `ALL`, `call` vs `call+type`) need a user decision.
- Rename's `--at` becomes `--offset`, and `RenameRequest.at` is renamed with it. The rename-arm doc
  comments have 9 lines and 10 occurrences of `--at`. The afterHelp exit-code text changes too.
- Text the rail cannot see goes into step 5:
  - `1_help.rs:14,16`
  - `1a_verb_help.rs:13,15,46`
  - `0_stratify.rs:277`
  - `0_graph.rs:854`
  - `_6_rename.rs:511-513`
  - the graph `flow_path` doc
  - the rename afterHelp
- The `-` split between path list and content bytes stays, documented in each arg's help.
- 23 multi-declared spellings, not the plan's 24.

### Still open (user decisions, not disagreements)

- The edit-verb corpus rule: full walk always, or keep tracked-only behind an explicit flag.
- The `Kinds` and `Arms` defaults once each concern has one parser.
- region `--apply` vs `--commit`. Neither of us proposed changing it; it is a candidate for the `Edit` model.
