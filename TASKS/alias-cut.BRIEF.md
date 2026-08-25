# alias-cut: delete the hidden verb aliases from boop

Repo: `/Users/chrishafley/projects/hafley-rs`. Crate: `crates/boop`. Work on a
branch `chore/alias-cut` in a worktree `.boop-worktrees/chore/alias-cut`.
Use your own target dir: `export CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target-alias-cut`.

## What breaks if this is wrong
`boop --help` says the folded verbs "still run as hidden aliases for one
release". 0.0.4 was that release. Every alias is a second path to one job;
the epic boop-one-path exists to have one path per job.

## Do exactly this, in order

### 1. Delete these `SubCmd` variants in `crates/boop/src/main.rs`
Each is marked `#[command(hide = true)]`. Delete the variant, its match arm
in `fn main`, and any helper only that arm called.

| variant | line (as of a4374c6) | replaced by |
| --- | --- | --- |
| `Codex` | 89 | `boop tui codex` |
| `TellParent` | 244 | `boop beep parent <body>` |
| `TellChildren` | 261 | `boop beep children <body>` |
| `Push` | 299 | `boop beep <route> <body>` |
| `Me` | 332 | `boop whoami` / `boop tui` |
| `Harnesses` | 348 | `boop db harnesses` |
| `Sessions` | 351 | `boop db` |
| `Tail` | 358 | `boop db` |
| `Events` | 369 | `boop db` |
| `List` | 375 | `boop beep lane list` |
| `Measure` | 385 | `boop db usage` |
| `Dispatch` | 391 | `boop beep lane create` |
| `Resolve` | 430 | `boop beep lane route` |
| `Hail` | 438 | `boop beep <route> <body>` |
| `Sweep` | 461 | `boop beep message ack` |
| `Lane` | 475 | `boop beep lane` |
| `Adopt` | 513 | `boop tui` |
| `Prune` | 544 | `boop beep lane delete --state dead` |
| `Chat` | 550 | `boop db chat` |
| `Sync` | 562 | `boop db sync` |
| `Follow` | 570 | `boop db` |

KEEP these hidden variants, they are not aliases:
`Concatmap` (187) and `Host` (237) are dl6-gated; `Inbox` (323) is called by
the installed claude Stop hook (`boop inbox drain`).

### 2. Delete these nested hidden variants
| enum | variant | line | replaced by |
| --- | --- | --- | --- |
| `BeepCmd` | `Hail` | 1318 | `boop beep <route> <body>` |
| `LaneCmd` | `Wait` | 1587 | `boop wait <lane>` |
| `MessageCmd` | `Ack` | 1734 | keep if `boop beep message ack` is the only spelling; delete only if it is an alias of another verb (read the doc comment) |

KEEP `LaneCmd::Run` (1476), the supervisor entry every lane pane runs.
KEEP every hidden `DbCmd` variant (1754..1872): they are the db read verbs
behind `db-four-verbs`, out of scope.

### 3. Remove dead code the deletions expose
`cargo build -p boop 2>&1 | grep -E "warning: (function|method|struct|enum|variant).*never (used|constructed)"`
must print nothing except `run_host` (dl6, pre-existing). Delete each named
item in `crates/boop/src/cli/*.rs`. Do not add `#[allow(dead_code)]`.

### 4. Fix the tests
`grep -rnE '"(push|hail|tell-parent|tell-children|adopt|prune|dispatch|resolve|sweep|measure|me)"' crates/boop/tests crates/boop/src`
Every hit is a test invoking a deleted alias. Rewrite each to the replacement
in the tables above with the same assertions. A test that only proved the
alias existed is deleted.

### 5. Fix the doctrine text
In `crates/boop/src/cli/mod.rs` and `crates/boop/src/main.rs` doc comments,
delete every line of the form `Folded aliases, hidden and unchanged: ...` and
the paragraph starting `The pre-split verbs (harnesses, sessions, ...`.
`boop --help | grep -ci "folded alias\|pre-split"` must print 0.

### 6. Validate
```
cargo fmt -p boop
cargo clippy -p boop -q 2>&1 | grep -c "^warning"      # must be <= 1
cargo test -p boop 2>&1 | grep "test result" | grep -v " 0 failed"   # must be empty
./target/debug/boop --help | grep -cE "^  [a-z-]+ " # must be 9
grep -c "hide = true" crates/boop/src/main.rs        # target: 16 (3 top-level + Run + 12 DbCmd); report the number you got
```
Run them from the worktree with `CARGO_TARGET_DIR` set. Paste all five
outputs in the final report.

### 7. Commit
One commit per step 1-5 is fine; every message starts with `alias-cut:`.
Do not merge, push, tag, or `cargo install`.

## Style laws
- No em dashes anywhere. No `provenance`, `substrate`, `load-bearing`,
  `regime` as words or identifiers.
- Doc comments state the fact; no "in reality", "actually", "obviously".
- Do not reformat code you did not change.

## Report (final message only)
| step | files | lines deleted | receipt |
plus the five validation outputs verbatim, and the list of tests rewritten
(old name -> new name).
