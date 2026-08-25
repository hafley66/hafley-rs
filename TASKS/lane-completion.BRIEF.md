# lane-completion: process exit and task outcome are two facts

Repo `/Users/chrishafley/projects/hafley-rs`. Branch `fix/lane-completion`,
worktree `.boop-worktrees/fix/lane-completion` (created for you).
`export CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target-lane-completion`.
Every test run: `BOOP_DB=$PWD/.scratch/boop.db HOME=$PWD/.scratch/home`
(mkdir them), never the real store.

Issue: `issues/lane-artifact-completion/item.md`. Read it first.

## What breaks if this is wrong
A lane whose harness exits 0 after an aborted provider stream, with no
commit and no report, reports `lane <id> done rc=0`. The parent reads
success. The receipt in the issue is one such lane.

## Design (fixed; do not redesign)
Expectations are typed flags on `lane create`, stored as one JSON file in
the lane's trail dir, evaluated by the supervisor at exit, and reported in
the result row.

```rust
// crates/boop-store/src/trail.rs
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct Expect {
    pub paths: Vec<String>,            // relative to the lane worktree
    pub commit_subjects: Vec<String>,  // exact subject line, any commit after base_sha
    pub commits_at_least: Option<u32>, // commits after base_sha
}
pub fn write_expect(lane: &str, expect: &Expect) -> Result<()>;   // ~/.agent/lanes/<lane>/expect.json
pub fn read_expect(lane: &str) -> Option<Expect>;                  // None when absent

// crates/boop-proc/src/supervise.rs
pub struct Unmet(pub Vec<String>);   // "missing path plans/x.md", "no commit with subject 'docs: ...'", "1 commit, expected at least 2"
pub fn evaluate_expect(cwd: &Path, base_sha: Option<&str>, expect: &Expect) -> Unmet;
```

Result row: `rc` stays the process exit. When `Unmet` is non-empty the row's
`detail` is `incomplete: <items joined by "; ">` and `rc` becomes `4` if the
process exit was 0 (a process failure keeps its own rc; the detail still
lists the unmet items). `4` is the "task incomplete" exit; document it in
the WAIT section of `crates/boop/src/cli/mod.rs` doctrine text next to 124
and 3. The parent's result row body already carries `detail` in parens
(`result_body`, supervise.rs:970), so `boop wait <lane>` prints it without
further change; confirm with a test.

## Steps, in order

1. `crates/boop-store/src/trail.rs`: `Expect`, `write_expect`, `read_expect`
   next to the existing `lane_dir` helpers. Round-trip test.
2. `crates/boop-proc/src/supervise.rs`: `evaluate_expect` (pure: reads the
   worktree with `std::fs` and `git -C <cwd> log --format=%s <base_sha>..HEAD`;
   no base_sha means every commit on HEAD counts, cap at 200). Call it
   inside `record_result` before the row is built (the lane's `base_sha`
   comes from `bus::read_routes(mail_dir)[lane].base_sha`). Tests: a temp git
   repo with one commit after base and one file; four cases: all met;
   missing path; wrong subject; too few commits. One test for the row: exit
   0 plus one unmet -> rc 4 and detail starts with `incomplete:`.
3. `crates/boop/src/main.rs` `LaneCmd::Create`: `--expect-path <REL>`
   (repeatable), `--expect-commit-subject <TEXT>` (repeatable),
   `--expect-commits-at-least <N>`. `crates/boop/src/cli/job.rs` lane create:
   when any is given, `write_expect` after the route is written. `--dry-run`
   prints an `expect:` line listing them. `run_lane_get` adds an `expect`
   field (the file's JSON or null).
4. Doctrine: `crates/boop/src/cli/mod.rs` WAIT section gets exit 4 and the
   three flags in the SPAWN section, two lines each at most. `boop --help`
   must still list 9 top-level verbs.
5. Tick the issue's boxes you met; `issuectl note lane-artifact-completion
   --as fix-lane-completion --agent-run "<what you ran>"`. Transcript
   terminal state (AC 3 wording) is met only through expectations; say so.
6. Validate, paste outputs:
```
cargo fmt -p boop -p boop-proc -p boop-store
cargo clippy -p boop -p boop-proc -p boop-store -q 2>&1 | grep -c "^warning"   # <= 1
cargo test -p boop -p boop-proc -p boop-store 2>&1 | grep "test result" | grep -v " 0 failed"   # empty
$HOME/.cache/boop/cargo-target-lane-completion/debug/boop beep lane create --help | grep -c expect   # 3
```
7. One commit per step, prefix `lane-completion:`. No merge, push, or install.

## Style laws
No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`. Doc
comments state the fact. Do not reformat untouched code.

## Report (final message only)
Table: step | files | receipt. Then the four validation outputs verbatim.
