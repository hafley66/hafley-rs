# ryi CLI: one flag, one model, one meaning

## Defect (2026-10-07, counted from schema/cli/ops.tsp)
- 24 flags hand-redeclared across ops (`--sqlite` 8 times).
- 3 spellings with two or more meanings: `--at`, `--from`, `--slow`.
- rename, cleave, move read files but take no `Inputs` (no PATH, --pattern, --entry).
- `--root` redeclared in 5 ops with its own type/default.

## Type sigs (domain.tsp)
```tsp
model Inputs   { @positional paths: path[]; patterns: string[]; entry: path[]; depth?: uint32; root?: path; }  // exists
model Store    { /** Write to a new SQLite database instead of stdout */ sqlite?: path; }
model Spans    { /** Add 1-based line/col beside every span */ lines?: boolean; }
model Scip     { scip_index?: path; scip_timeout?: uint64; }
model Checkers { @requires("root") rust_checker?: boolean; ts_checker?: boolean; go_checker?: boolean; }
model Receivers{ @valueName("written") fast_receivers?: string; }
model Revision { /** Query a committed revision */ rev?: string; /** Diff against REV */ compare?: string; }
model Edit     { list?: path; state?: path; commit?: boolean; verify?: string; text_refs?: boolean; json?: boolean; }
```
Ops become spreads plus op-only flags:
```tsp
op graph(...Inputs, ...Store, ...Scip, ...Checkers, ...Receivers, ...Revision, callers?, uses?, from?, ...): ...
op rename(@positional target?: string, @positional new?: string, ...Inputs, ...Edit, offset?: uint32, slow?: boolean, ...): ...
op cleave(@positional target?, @positional dest?, ...Inputs, ...Edit, drag?, slow?): ...
op `move`(@positional old?, @positional new?, ...Inputs, ...Edit, shim?, relocate_mod?): ...
op diff(...Inputs, ...Store, ...Revision, arms): ...            // --from/--to REV become --rev/--compare
op stratify(...Inputs, rank: string[], base_lines?, kind?): ... // --from PATH[:NAME][=RANK] becomes --rank
```

## One meaning per spelling
| spelling | today | after |
|---|---|---|
| `--at` | graph: revision; rename: byte offset | gone: revisions are `--rev`; rename's byte offset is `--offset` |
| `--from` | graph: seed; diff: revision; stratify: entry | graph seed only; diff uses `--rev/--compare`; stratify uses `--rank` |
| `--slow` | graph: SCIP edges; cleave/rename: compiler plan | compiler tier everywhere; graph's SCIP walk is `--scip-index`/`--oracle scip` |
| `--root` | 6 declarations, `move` takes a list | `Inputs.root` only; move's extra roots become PATH inputs |

## Handler change (hand-written)
rename/cleave/move: the corpus is `inputs::files(&Inputs)`, the same function graph uses.
No PATH given = today's behavior (whole `--root`). PATH given = occurrences only in those files.

## Rail (one test, table-driven over the real clap tree)
Walk `help_command()` subcommands; for every long flag collect (value_name, action, help).
A spelling with more than one distinct tuple fails, naming the ops. Today it fails on
`at`, `from`, `slow`, `root` and every doc drift among the 24 copies.

## Order
1. Rail test, failing, snapshot of today's collisions.
2. domain.tsp models + ops.tsp spreads; regenerate; rail lists only the 3 renames.
3. Renames (`--rev`, `--offset`, `--rank`, diff `--rev/--compare`, graph SCIP flag); rail green.
4. rename/cleave/move handlers honor `Inputs`.
5. Help snapshot, full suite, push.

## Decisions after review (user, 2026-10-07)
- Reviews: `*.review-a.md`, `*.review-b.md` (3 rounds, both critics agree on all technical points).
- Revisions: `--rev REV` or `--rev BASE..TARGET` (base left). `--rev A` alone on diff, `A..`, `..B`, `A...B`: exit 2.
- Edit corpus: rename/cleave/move use the .gitignore-aware walk (`inputs::expand`) by default; `--tracked` narrows to `git ls-files`.
- `--apply` vs `--commit`: one spelling per action. rename/cleave/move `--commit` and region `--apply` all
  write files through a Soopy stage; none makes a git commit. Spelling: `--apply` everywhere.
- `--kinds` / `--arms` defaults: open (see chat 2026-10-07).

## Landed (2026-10-07)
- Rail: tests/218_flag_consistency.rs, snapshot empty (every long flag one shape across verbs).
- Models: Store, Spans, Edit (state, apply, text_refs, tracked) in domain.tsp.
- `--rev REV|BASE..TARGET` on graph and diff; `--at`/`--compare`/`--from`/`--to` gone.
- `--apply` replaces `--commit`; rename `--offset` replaces `--at`; rename `--scip-index` replaces `--verify-scip`.
- stratify `--rank PATH[#NAME][=RANK]` (the critics proposed keeping `--from`; the rail rejects one spelling with two shapes).
- rename/cleave/move: `...Inputs` (PATH scope, --pattern, --entry), stdin body, .gitignore walk by default, `--tracked`.
  With `--list`, positionals are PATH scope. move `--roots DIR...` for several corpora.
- Fast Rust rename: unresolved path prefixes, files without a module home, and macro-body
  `a::b::name` now abstain or resolve instead of dropping uses silently.
