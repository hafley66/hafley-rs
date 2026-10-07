# Review A: ryi CLI flag models

## Round 1

Read in full: the plan, `schema/cli/domain.tsp`, `schema/cli/ops.tsp`, everything under
`crates/ryi-proto/src/gen/` and `crates/ryi/src/gen/`, `crates/ryi-proto/src/1_help.rs`,
`1a_verb_help.rs`, `crates/sprefa-extract/src/bin/ryi.rs` (`run`, `run_formatted`, `run_verb`),
`bin/ryi/{0_cli,0_fast_receivers,0_revision,1_inputs,ops}.rs`, `5_diff.rs`, `0_graph.rs`,
`0_stratify.rs`, `4_watch.rs`, `edit/_6_rename.rs`, `edit/_6_move.rs`. From
`edit/_7_cleave.rs` (3190 lines) I read only the arg-handling paths: `run`, `run_list`,
`Plan::build`, `single`, `plan_root`. From `3_region_writer.rs` and `0_query.rs` I read only
their arg reads.

### 1. The comparison flag: `--rev A --compare B` vs `--rev A..B`

What the code does today:

| verb | flags | base side | target side | worktree side | output fields |
|---|---|---|---|---|---|
| graph | `--at A --compare B` | A (`before`, 0_graph.rs:850) | B (`after`, :856) | none; `--at` absent = worktree, no compare | `graph_path_change{change, revision}`; `removed` tagged A, `added` tagged B (:743-766) |
| diff | `--from A --to B`, both required | A (5_diff.rs:48) | B (:49) | none; `RevisionReader` takes commits only (0_revision.rs:42) | `diff_run{from,to}`, `from_digest/to_digest`, `from_origin/to_origin`; SQLite `from_sha/to_sha` |

Both verbs use the same direction, A is the base and B is the target. graph's help says the
opposite: `compare` is documented as "Diff the answers against REV" (ops.tsp), which makes B
sound like the baseline. `--compare B` doesn't say which side B is on, and the doc already got
it backwards once.

Candidates:

| spelling | flags per concept | direction encoded | clap-native constraints | git precedent | parse cost |
|---|---|---|---|---|---|
| `--rev A --compare B` | 2 | no (word order of flags) | `compare requires rev` | none | none |
| `--rev A..B` | 1 | yes, left is base | `requires` is implicit; path-arm restriction stays runtime (it already is, 0_graph.rs:853) | `git diff A..B` = `git diff A B` (endpoints) | split on `..`; `..` is illegal inside a refname (git check-ref-format), so the split is unambiguous; reject `A...B` |
| `--rev A --rev B` (repeatable, ordered) | 1 | yes, by order | needs a max of 2; the Clap emitter has `@minItems` (ops.tsp:345) but no `maxItems` precedent | `git diff A B` | none |

My pick is `--rev A..B`. Reasons:
- It gives one spelling for one concept. `Revision` becomes `{ rev?: string }` with
  `@valueName("REV|A..B")`, and every verb that spreads it gets both forms.
- graph `--rev A` keeps its current single-revision meaning. `--rev A..B` replaces
  `--at/--compare` without changing behavior.
- On diff, `--rev A` alone is a named exit 2 ("diff needs A..B"), because a worktree side does not
  exist in `RevisionReader`. If a worktree side lands later, `A..` can mean "A to worktree"
  without adding a third flag.
- `--compare` is used in 1 test (`tests/168_graph_revision.rs:28`) and in the
  `cli_http_parity/0_contract.json` fixture. The migration is small either way.

Fallback: `--rev A --rev B`, if the emitter grows `@maxItems`.

Cross-system naming (AGENTS.md casing rule): the diff wire already says `from`/`to`.
`--rev/--compare` would add a third vocabulary (`rev`, `compare`, `from`, `to`) for two sides.
With `A..B`, the request field is `rev` and the output sides keep `from_*/to_*`. The output then
describes the sides, while the flag describes the range. The plan should state which way it
goes. `from` on the wire already carries 2 meanings: a revision side in diff, and an edge
endpoint in `graph_edge.from_path` and `stratify_move.from_path`.

### 2. `--from`: graph and stratify are one concept

The plan's table says `--from` has 3 meanings. By the handler code it has 2:
- graph `--from [PATH#]NAME` seeds a reach walk (`named_starts`, 0_graph.rs:368).
- stratify `--from PATH[:NAME][=RANK]` seeds a reach walk too (`reachable_depths`,
  0_stratify.rs:319).
- Only diff's `--from REV` is different.

Renaming stratify's flag to `--rank` names it after an optional suffix. The real inconsistency
is the separator: `#` in graph (and in `rename FILE#OLD`, `cleave SRC#ITEM`) against `:` in
stratify. Counter-proposal: keep `--from` on both, with stratify taking `PATH#NAME[=RANK]`.
That leaves diff's `--from` as the only one that moves, into `--rev`.

stratify also spreads `Inputs`, so it already has `--entry FILE`, a second seed-like flag meaning
"filter to files reachable over imports" (1_inputs.rs:69-80). Two seeds on one verb need one
line of help each.

### 3. `--slow`: the collision is smaller than the plan claims

graph `--slow` without `--scip-index` calls `slow_project(paths, root, None, true)`
(0_graph.rs:118), and the `true` means checkers on. For `--callers`/`--uses` it calls
`target::facts` (:116). So graph's `--slow` already means "compiler/SCIP tier", the same
meaning as the `ryi slow` verb. rename's `--slow` (TS7 checker / rust-analyzer) and cleave's
(rust-analyzer / TS LSP) are the compiler tier too. The difference across the 3 declarations is
help text and which engine each language gets, not two meanings.

Moving graph's flag to `--scip-index`/`--oracle scip` makes the verb `ryi slow` and the flag
`--slow` diverge. The "compiler tier" concept already has 3 spellings:
- `--rust-checker/--ts-checker/--go-checker` (FileArgs, graph)
- `--slow` (graph, rename, cleave)
- `ryi slow` + `--no-checker` (slow verb)

The plan's `Checkers` model leaves those 3 spellings in place. I'd keep `--slow` on all 3 verbs,
and settle in this plan whether the per-language checker booleans survive next to it.

### 4. `--root` on move: PATH inputs collide

`move --root` is repeatable, and each root is its own corpus:
- `Plan::build_multi` (_6_move.rs:228-286) builds one `MoveCx` per root and assigns each row to
  exactly one root.
- Output gets a `[root X]` prefix.
- A failed `--verify` rolls every root back (:177).
- `--verify-cwd` defaults to the first root.

The plan's "Order 4" makes PATH on move mean "occurrences only in those files". Its table makes
move's extra roots into PATH inputs. Those are two meanings for one positional on one verb.
One of them has to go, or move keeps `root: path[]` as an op-level field, and the rail
whitelists it by name.

Two related root details the plan doesn't cover:
- The edit verbs read whether `--root` was given: `open_with_untracked(&root,
  cli.root.is_some())` in rename (_6_rename.rs:183) and cleave (_7_cleave.rs:172, :738). Passing
  `--root` adds untracked files. `Inputs.root` keeps `Option`, so this survives, but the
  behavior belongs in the help.
- Defaults differ. Edit verbs use the git root of the anchor file (rename `plan_root` :572,
  cleave :2959). watch/diff use the git root of the cwd (`git_root_of_cwd`). `inputs::root` uses
  `--root`, then the single dir input, then the cwd git root. A shared `Inputs.root` doc ("Corpus
  root") would drop those three defaults from help.

### 5. Revision × Inputs interaction is unspecified

graph under `--at` reads `Inputs.paths` as path-prefix filters (`selected`,
0_graph.rs:826-848). It ignores `--pattern` and `--entry` and uses `default_patterns()` (:697).
Once diff spreads `Inputs` (as the plan proposes), its new PATH positional and `--entry` need the
same rule. Right now `--pattern` would apply to diff but not to graph `--at`. The plan should say
which `Inputs` fields a revision run honors, and refuse the rest with exit 2. Silently dropping
them is the current graph behavior.

### 6. Other cross-verb drift (not in the plan's table)

| concept | spellings / declarations | file:line |
|---|---|---|
| resolve arms | FileArgs `--arms` (call,type,flow; default call); diff `--arms` (call,type; default call+type); stratify `--kind` (call\|type\|both) | ryi.rs:979, 5_diff.rs:149, 0_stratify.rs:50 |
| fact families | FileArgs `--kinds` (default `DEFAULT`, accepts cfg); watch `--kinds` (default `ALL`, rejects cfg, no valueName) | ryi.rs:998, 4_watch.rs:533 |
| apply vs dry run | `--commit` (cleave, move, rename); `--apply` (region) | ops.tsp region |
| soopy state | `--state` (cleave, move, rename, region); watch `--receipts` feeds `Options.state` | 4_watch.rs:501 |
| `--verify*` prefix | `--verify CMD` (cleave, move); `--verify-cwd DIR` (move); rename `--verify-scip INDEX` is an index, which is `--scip-index`'s meaning | ops.tsp rename |
| JSON output | `--json` (cleave, rename) plus hidden global `--format jsonl`; move has neither | |
| `--list` conflicts | in clap for cleave only; runtime for move (_6_move.rs:507) and rename (_6_rename.rs:518) | |
| stdin `-` | Inputs: path list; ingest and region `--generated`: content bytes; `request_uses_stdin` treats both the same | 1_inputs.rs:20, ryi.rs:1161 |
| `--sqlite` help | 7× "Write to a new SQLite database instead of stdout"; graph "Keep the fact store in a new SQLite database at PATH" | |
| `--scip-index` | 4 declarations, 3 help texts; `requires root` in clap on graph, at runtime on extract (ryi.rs:629, :757) | |

Duplicate implementations, which the "one implementation per concern" rule in CLAUDE.md covers:
- `parse_mask` (ryi.rs:998 / 4_watch.rs:533)
- `parse_arms` (ryi.rs:979 / 5_diff.rs:149)

A shared `Arms`/`Kinds` model only gets one meaning if each pair also collapses to one parser.

### 7. Plan mechanics

- **Edit model.** `@conflictsWith(#["target","dest"])` on `list` can't live in a shared model,
  because the positional names differ per op: target/dest, old/new, target/new. Either rename
  the positionals to one pair, or keep the conflict at the op level.
- **Edit model adds behavior.** It adds `verify` to rename and `json` to move, and neither handler
  implements them. Region is left out of `Edit` even though it has `state` and a commit-like
  switch.
- **Rail coverage.** The rail walks `help_command()`, but the TOP text (1_help.rs:4) and the
  EXAMPLES (1a_verb_help.rs) are hand-written. They hardcode `diff --from <REV> --to <REV>`,
  `stratify --from <ENTRY>` and `diff --from HEAD~1 --to HEAD`. The rail won't see them, so step
  5 should name both files.
- **rename `--at` to `--offset` touches more than the CLI.** It also reaches
  `RenameRequest.at` (_6_rename.rs:612) and the doc comments in `rust_rename.rs`,
  `ts_rename.rs`, `prolog_rename.rs`, which say "`--at`" in 7 places. It also reaches
  `afterHelp "3 ambiguous (pass --at)"`. The casing rule implies renaming the field too.
- **Test churn.** 4 test files plus 5 rename fixtures pass `--at` as an offset. 7 graph files or
  fixtures pass `--from` as a seed, and that churn goes away under §2.
- **Plan wording.** The plan says "3 spellings with two or more meanings", but its table lists 4
  (`--root` included). `--sqlite` ×8 checks out: FileArgs, fast, slow, scip, graph, query, diff,
  ingest.

## Round 2

I read B's Round 1 in full, and spot-checked its claims in `client_auto.rs`, `server_auto.rs`,
`_6_move.rs`, `_7_cleave.rs` and `ops.tsp`.

### Agreements (no change to round 1)

| topic | A §  | B §  | what we both found |
|---|---|---|---|
| `--rev A..B` | 1 | 1 | Same evidence (`0_graph.rs:850-868`, `5_diff.rs:48-49`), same direction (A base), same `..` refname argument, same `...` rejection, same diff `--rev A` = exit 2. |
| `--slow` | 3 | 2 | It is one tier, so keep `--slow` on graph/rename/cleave and fix graph's help. B also traced `0a_graph_target.rs` and `0c_graph_walk.rs`, which I hadn't read, and both are compiler-tier. That makes it stronger. |
| stratify seed | 2 | 6 | Keep `--from`, with `#` as the one anchor separator. B adds that the current `:` also collides with graph `--slice PATH:BYTE`, which is a third reason. |
| move `--root` | 4 | 5 | Multi-root collides with "PATH filters occurrences". |
| revision × Inputs | 5 | 5 | `--pattern/--entry` are silently ignored under a revision. |
| non-clap flag text | 7 | 7 | B's list is longer than mine. Add `0_stratify.rs:277`, `0_graph.rs:854`, and the `1a_verb_help.rs:46` `"from"` id key. |

### Where B changes my position

1. **The rail key.** In round 1 I took the plan's `(value_name, action, help)` tuple as given. B
   §4 shows it can't go green, because `--list`, `--json`, `--text-refs`, `--timeout` and `--state`
   differ in help for real reasons. I now agree with B:
   - The rail keys on `(value_name, action, value type)`.
   - A spread-model flag can't drift in help, because it has one doc.
   - Op-local flags that share a spelling get an explicit allowlist row each.

   Keying on value_name still catches `--root`. Inputs declares `DIR`, but cleave, move and
   rename declare no valueName, so clap prints `ROOT`. That is a real catch.
2. **`--sqlite` on graph is a meaning collision.** I filed it under help drift. B §3 is right:
   graph writes the store and still emits answers to stdout. `load_store` gets the sqlite path at
   `0_graph.rs:880`, and `emit_rows` writes to `output` at `0_graph.rs:886`. The other 7 copies
   write to SQLite *instead of* stdout. I agree with B's neutral `Store` doc ("New SQLite database
   at PATH"), with stdout behavior described in each verb's about text. That beats a new
   `--keep-store` spelling, which would give "new SQLite DB" a second flag name.
3. **stdin over the daemon.** This is new to me and it blocks the plan's step 4. I verified B's
   claim and corrected one detail:
   - `request_uses_stdin` returns `&[]` for cleave, move, rename, diff, watch and stratify
     (`daemon_auto.rs:49-68`).
   - The client streams stdin only for the 8 `@bodyRoot` verbs (`client_auto.rs:187`).
   - The correction: the daemon is spawned with `.stdin(Stdio::null())` (`client_auto.rs:39`). So
     in `inputs::expand`, `-` with `request_input_file() == None` reads empty
     (`1_inputs.rs:26-30`). It does not hang.
   - So `ryi stratify -` already silently expands to nothing today, and the same would happen to
     rename, cleave and move after the spread.
   - Fix: every op that spreads `Inputs` gets `@bodyRoot input: RawByteStream`, stratify
     included. A rail row can enforce "spreads Inputs ⇒ has bodyRoot" from `request_uses_stdin`.
4. **The revision root rule.** graph `--at` has `@requires("root")` (ops.tsp:141) and
   canonicalizes it (`0_graph.rs:823`), while diff defaults to `git_root_of_cwd`
   (`5_diff.rs:139`). One `--rev` should have one root rule. I'll take diff's default and drop
   `@requires("root")` from the revision path.

### Where I differ from B, or add to B

- **move vs cleave/rename untracked behavior.** B says `MoveCx::open_with_untracked` is used by
  "rename, cleave and move". From the code:
  - rename uses `RenameCx::open_with_untracked(.., cli.root.is_some())` (`_6_rename.rs:183`).
  - cleave uses `MoveCx::open_with_untracked(.., cli.root.is_some())` (`_7_cleave.rs:172, 738`).
  - move uses plain `MoveCx::open(&root)` (`_6_move.rs:293`).

  So passing `--root` adds untracked files on 2 of the 3 edit verbs, and never on move. After the
  spread, one `Inputs.root` has two side effects. The plan has to pick one: move gains the
  untracked switch, or rename and cleave lose it. A separate flag would be a third option.
- **Option B gave for move's roots: "`Inputs.root` becomes `path[]`, every other verb rejects
  >1."** I'm against it. The ">1" rejection can't be expressed in the Clap emitter: there's no
  `@maxItems`, and `@minItems` (ops.tsp:345) is the only items decorator. So it becomes a runtime
  check in every verb except move, and the rail can't see it. I'd keep move's `root: path[]`
  op-local and allowlisted, and the rail flags the spelling once. B's other option, `--roots` on
  move, is fine too and is clap-visible. I lean `--roots`, because it removes the allowlist row.
- **Duplicate parsers.** B notes the `--kinds` and `--arms` drift as help differences. I'd keep
  it as a code-level point. `parse_mask` is implemented twice (`ryi.rs:998`, `4_watch.rs:533`),
  and so is `parse_arms` (`ryi.rs:979`, `5_diff.rs:149`). The defaults also differ: `DEFAULT`
  vs `ALL`, and `call` vs `call+type`. A shared model with two parsers keeps two meanings, so
  the model change has to land together with collapsing each pair to one parser. Otherwise the
  rail goes green while behavior still forks.
- **Counts.** B counts 23 multi-declared spellings, the plan says 24, and I didn't recount. B's
  23 is the only count backed by a listed method, so I'd use it.

### Open between us

- **diff `--rev A` alone.** B allows `A..HEAD` as a possible default. I'd keep exit 2. `A..`
  should stay reserved for A-to-worktree, which `RevisionReader` can't do yet
  (`0_revision.rs:41-43`), and if `A..` means HEAD now, giving it a worktree meaning later breaks
  scripts.
- **Who owns `-` semantics.** Neither of us proposed a fix for `-` meaning a path list in Inputs
  but content bytes in ingest and region `--generated`. `request_uses_stdin` already treats both
  identically. I'd leave ingest and region alone and document the split in each verb's help, but
  that is a choice for the user.

## Round 3

I re-read `review-b.md` in full, including B's Round 2. Before the positions, a correction to my
own Round 2.

**Correction.** In Round 2 I wrote that move "never" includes untracked files. That is backwards.
`MoveCx::open` calls `walk_files(root)`, which is `walk_files_with_untracked(root, true)`
(`edit/_1_move_cx.rs:80-84, 258-260`), so move **always** includes untracked files.

rename and cleave call `open_with_untracked(root, cli.root.is_some())`
(`_6_rename.rs:183` -> `_1_rename_cx.rs:62-65`, and `_7_cleave.rs:172, 738`). With
`include_untracked == false`, the corpus is `git ls-files -z`, which is tracked files only
(`_1_move_cx.rs:268-284`).

So the real split:

| verb | `--root` absent | `--root` given |
|---|---|---|
| move | tracked + untracked | tracked + untracked |
| rename, cleave | tracked only | tracked + untracked |
| read verbs (`inputs::expand`, ignore-aware walk, `1_inputs.rs:118-130`) | tracked + untracked non-ignored | tracked + untracked non-ignored |

### Final positions on the open points

1. **diff `--rev A` alone: exit 2.** B's Round 2 point (Disagree 2) moves me here. Empty sides
   are also exit 2 now: `A..`, `..B` and `A...B` all reject. Nothing is reserved in the help
   text, so the help never advertises a form the handler rejects. A worktree side would be new
   syntax added together with new `RevisionReader` work (`0_revision.rs:41-43`). I also accept
   B's Disagree 1: drop the `--rev A --rev B` fallback from the plan. It encodes direction by
   argv order and needs a `@maxItems` that doesn't exist.

2. **What `-` means.** In this plan it means a path list everywhere `Inputs` is spread
   (`1_inputs.rs:20-38`). The content-bytes `-` stays on ingest's positional (`ryi.rs:1161`) and
   region's `--generated` (ops.tsp region). Neither of those spreads `Inputs`, so no single flag
   carries two meanings. The split sits between two different arguments. Two concrete
   requirements:
   - Every op that spreads `Inputs` gets `@bodyRoot input: RawByteStream`. This covers stratify
     today, plus rename, cleave, move and diff after the plan. Otherwise `-` reads the daemon's
     closed stdin (`client_auto.rs:39`) and silently expands to nothing.
   - The help for ingest PATH and region `--generated` states "file content" in words.

   No rename of either flag.

3. **move `--roots DIR...` op-local, with `--root` singular from `Inputs`.**
   - `--roots` carries the multi-root batch: one plan per root, rows assigned by prefix,
     rollback across all roots (`_6_move.rs:24-29, 228-286`).
   - It declares `@conflictsWith(#["root"])`. That names an `Inputs` field, so it stays legal
     under B's constraint rule (point 5).
   - `--verify-cwd` default text changes to "first of --roots, else --root".
   - Clap sees all of it, and the rail sees two distinct spellings with no allowlist row. I
     reject `Inputs.root: path[]` for the reason in Round 2: there's no `@maxItems`, so every
     other verb would need a runtime check.

4. **The untracked-files side effect of `--root`: remove it.** Whether `--root` is present should
   select the root and nothing else. Proposal:
   - All three edit verbs take their corpus from the same walk as the read verbs: ignore-aware,
     untracked non-ignored files included. This matches move's current behavior and plan step 4's
     `inputs::files(&Inputs)`.
   - rename and cleave without `--root` gain the untracked files they lose today. That is a
     behavior change, and the plan's handler step should list it with a fixture: an untracked
     importer gets respelled.
   - If the user wants tracked-only planning, that is its own flag shared by all three, not a
     side effect of `--root`. I'm not proposing one here, since no test or fixture asks for it.

5. **Edit model constraint rule: adopt B's.** A constraint inside a shared model may name only
   fields of the same model or of `Inputs`. Everything else is declared at the op level. The
   cases this forces, from ops.tsp:
   - `list @conflictsWith(target, dest)` stays per op. The positionals differ: cleave
     `target/dest`, move `old/new`, rename `target/new`. clap's debug asserts reject an unknown id
     in `conflicts_with`, so a shared model would fail at `help_command()`, which is B's point.
   - graph's `at @conflictsWith(rust_checker, ts_checker, go_checker, scip_index, sqlite)`
     (ops.tsp:141) names `Checkers`, `Scip` and `Store` fields. Under `Revision` it must stay on
     graph's op. diff spreads `Revision` without `Checkers`, so carrying the constraint in
     `Revision` would break diff the same way.
   - graph's `slow @conflictsWith(checkers)` stays per op (B's Disagree 3).
   - `Checkers @requires("root")` and `Inputs.depth @requires("entry")` are legal. The first is
     only legal when spread next to `Inputs`, so the rail asserts "spreads `Checkers` ⇒ spreads
     `Inputs`".

   I'd also add one rule of my own: a shared model never adds a flag to an op whose handler
   doesn't read it. So `Edit` drops `json` and `verify` (or they become op-local). Today `json`
   exists on cleave and rename and `verify` on cleave and move, and `rename.verify` /
   `move.json` have no handler (`_6_rename.rs:64-131`, `_6_move.rs:31-119`).

   I accept B's on the per-language checker booleans: they keep one meaning each and stay next
   to `--slow`, outside this plan.

### A and B now agree on

- `--rev` is one flag: `REV` or `BASE..TARGET`, base on the left. `--at`, `--compare`, and
  diff's `--from`/`--to` all go away. Empty sides and `...` exit 2. diff requires the range.
  The direction is pinned by `tests/168_graph_revision.rs:28-44`, and the reversed help is
  frozen in `tests/fixtures/ryi_help/graph.txt:26`.
- Output wire fields keep `from_*/to_*` (`diff_run`, `from_sha/to_sha`) and graph's `revision`.
  The request field is `rev`. The plan says so explicitly.
- One root rule under `--rev`: diff's git-root-of-cwd default. Drop graph's `@requires("root")`.
- `--slow` stays on graph, rename and cleave as one tier, with one help string.
- stratify keeps `--from`, with `#` as the one anchor separator. No `--rank`.
- The rail keys on `(value_name, action, value type)`. Help is checked per model. Op-local
  same-spelling flags get explicit rows.
- graph `--sqlite` differs in meaning. `Store`'s doc becomes neutral, and stdout behavior goes
  in each verb's about text.
- Under `--rev`, `Inputs` fields the handler doesn't read exit 2 (`0_graph.rs:697, 826-848`,
  `5_diff.rs:88`).
- `@bodyRoot` goes on every op that spreads `Inputs`, stratify included.
- `parse_mask` and `parse_arms` collapse to one parser each (`ryi.rs:979, 998`;
  `4_watch.rs:533`; `5_diff.rs:149`), with the two default sets decided by the user.
- Flag names in prose outside clap get updated by name:
  - `1_help.rs`, `1a_verb_help.rs:13, 15, 46`
  - the ops.tsp `flow_path` doc and rename `afterHelp`
  - `_6_rename.rs:32, 513`, `0_stratify.rs:277`, `0_graph.rs:854`
  - 10 `--at` occurrences in `rust_rename.rs`, `ts_rename.rs` and `prolog_rename.rs`, by B's
    count
  - `RenameRequest.at` becomes `offset` (casing rule)
- The multi-declared spelling count is 23 (B's method), not 24.
