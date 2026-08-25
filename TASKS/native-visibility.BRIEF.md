# native-visibility: every live agent shows up, and "live" is measured

Repo `/Users/chrishafley/projects/hafley-rs`, crate `crates/boop` (plus one
test in `crates/boop-store`). Branch `fix/native-visibility`, worktree
`.boop-worktrees/fix/native-visibility` (already created for you).
`export CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target-native-visibility`.
Every test run: `BOOP_DB=$PWD/.scratch/boop.db HOME=$PWD/.scratch/home` set
first (mkdir them), never the real store.

Issues: `issues/boop-native-subagents-invisible/item.md` and
`issues/live-lane-session-graph/item.md`. Read both before editing.

## What breaks if this is wrong
`boop beep lane list` says `live` for a pane-less route forever
(`lane_state`, `crates/boop/src/cli/job.rs:1531`: `route.tmux.is_none() &&
kind in {coordinator,native}` returns "live" with no check). Two dead
native routes sat there for a day today. Claude Code Agent-tool subagents
(`<repo>/.claude/worktrees/agent-*`) and tmux sessions with no boop route
appear nowhere, so the user's cmd+period view and instant's panel undercount.

## Steps, in order

### 1. Measured liveness for pane-less routes (`crates/boop/src/cli/job.rs`)
Replace the hardcoded `return "live"` in `lane_state` with:
- route has `parent`: state of the parent route (recurse once, no deeper;
  a parent that is itself pane-less and parentless is `?`). A dead parent
  makes the native `dead` with suffix `PARENT-GONE=<parent>` (the suffix
  mechanism already exists in `run_lane_list`, reuse it).
- route has no `parent` and no `tmux`: `?`.
Unit test in the existing `mod tests` of job.rs: native with live parent ->
live; native with dead parent -> dead; parentless pane-less -> `?`.

### 2. Unregistered tmux sessions in `lane list` (`crates/boop/src/cli/job.rs`)
After the registry loop in `run_lane_list`, list every tmux session from
`tmux::mux().live_sessions(None)` whose name matches no route `tmux`
target and no route name. Print one row each: state `live`, name = tmux
session name, kind `unregistered`, other columns `-`. Hidden unless
`--all` (add the flag to `LaneCmd::List` in `crates/boop/src/main.rs` if it
is not there; `--all` already exists on `beep ps`, mirror its doc comment).
Test with the fake mux the file already uses (grep `FakeMux` or `Mux` trait
in `crates/boop-mux`); if no fake exists, test the pure function you
factor out (`unregistered_sessions(routes, live) -> Vec<String>`).

### 3. Claude Agent-tool subagents in `lane list` (`crates/boop/src/cli/job.rs`)
For every registered route with `harness == Some(Claude)` and a `cwd`,
run `git -C <cwd> worktree list --porcelain` and take each worktree whose
path contains `/.claude/worktrees/agent-`. Print one row each: kind
`native-claude`, name = the `agent-<id>` directory name, parent = that
route's name, worktree column = the path, state `live` when the porcelain
block has a `locked` line, else `dead`. Also under `--all` only. Factor
`claude_agent_worktrees(cwd) -> Vec<(name, path, locked)>` and unit test
it against a temp git repo with one locked and one unlocked linked
worktree (`git worktree add`, `git worktree lock`).

### 4. Session graph receipt (`crates/boop-store/src/_0_session_graph.rs`)
Add one test in its `mod tests`: a runtime row with
`route.kind == "lane"`, `route.harness == Some(Opencode)`,
`route.session_id == None`, a tmux target, `liveness.tmux == Live` goes
through `load_agent_session_graph_with_runtime` and appears exactly once in
`graph.shells` with `state == "live"` and `session == None`; a second call
after the same lane's session resolves (route.session_id set and a matching
`agent_session` row) shows it once in `sessions` and zero times in
`shells`. If the second half fails, fix `shell_from_runtime` (line 430) so
a route whose session is present in `sessions` is dropped from `shells`.
Use the fixture helpers already in that test module (grep `AgentRuntimeRow {`
near line 1261).

### 5. Check the boxes
In both issue files, tick every `- [ ]` you satisfied and leave the rest;
`issuectl note <slug> --as fix-native-visibility --agent-run "<what you
ran>"` once per issue (run `issuectl` from the worktree root). AC 8 of
live-lane-session-graph (instant fixture) is out of scope: say so in the
note.

### 6. Validate (paste all outputs in the report)
```
cargo fmt -p boop -p boop-store
cargo clippy -p boop -p boop-store -q 2>&1 | grep -c "^warning"   # <= 1
cargo test -p boop -p boop-store 2>&1 | grep "test result" | grep -v " 0 failed"   # empty
./target/debug/boop beep lane list --all --mail-dir $PWD/.scratch/mail | head   # runs
```
(`CARGO_TARGET_DIR` above means the binary is at
`$HOME/.cache/boop/cargo-target-native-visibility/debug/boop`.)

### 7. Commit
One commit per step, message prefix `native-visibility:`. No merge, no
push, no `cargo install`.

## Style laws
No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` as
words or identifiers. Doc comments state the fact. Do not reformat code you
did not change. Do not touch `crates/boop-proc`.

## Report (final message only)
Table: step | files | receipt. Then the four validation outputs verbatim.
