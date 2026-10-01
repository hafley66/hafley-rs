# Burndown item 3: migrate-mux-slice (issue `migrate-mux-slice`)

Goal: tmux is spawned only inside crates/boop-mux, and the boop tsp `mux/` slice fully describes boop-mux.

## Rust (this lane's hafley-rs worktree)
The 5 `Command::new("tmux")` outside boop-mux (ryii effect inventory, 2026-10-01):
- crates/boop/src/cli/job.rs:3861 (attach-session, honors route.socket)
- crates/boop/src/cli/selection.rs:170 (list-panes -a -F <format>)
- crates/boop-proc/src/deliver.rs:250 (send-keys -l literal) and :258 (send-keys key)
- crates/boop-proc/src/lane.rs:558 (list-panes)
Route each through the `boop_mux::Multiplexer` trait (crates/boop-mux/src/lib.rs:95; it already has send_text,
send_keys_literal, send_key_named, list_panes). Add trait methods only where none fits (e.g. attach). Same argv and
socket behavior; no behavior change. Rules: CLAUDE.md (small numbered new files; never grow a >1000-line file —
job.rs is 6646 lines, so new code goes elsewhere; one implementation per concern). Build with
`CARGO_BUILD_JOBS=4`; one cargo build at a time. `cargo test -p boop-mux -p boop-proc --lib`, and
`cargo test -p boop --bin boop`.

Ownership check (must be 0 rows): rebuild facts and query, from the worktree root:
`crates/sprefa-extract/target/release/ryii --root . --resolve --rust-checker --kinds call --sqlite <db> crates/boop*`
then the SQL in plans/boop-effects/0_test_scope.sql + 0a_effects.sql (copy the db path), then
`select path, line from effect_site where callee='std::process::Command::new' and written_after like '("tmux"%' and crate<>'boop-mux'`.
Use the existing ryii binary under /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/writer-ds2/crates/sprefa-extract/target/release/ryii; do not build ryii.

## boop tsp (boop2)
`git -C /Users/chrishafley/projects/boop2 worktree add /Users/chrishafley/projects/boop2-mux -b burndown/mux main`.
In `schema/mux/`: add `union ControlEvent` (BlockBegin{num}, BlockEnd{num}, BlockError{num}, Body(string),
Notification) and `union Notification` (Output{pane,text}, SessionChanged{id,name}, WindowAdd{id}, Exit, Unknown(string))
from boop-mux lib.rs:811-830; `interface ControlClient` (command, next_event) from lib.rs:889; and the tmux argv
contract: one op per tmux subcommand boop calls (list-panes, list-sessions, send-keys, display-message, new-session,
capture-pane, swap-window, split-window, paste-buffer, new-window, load-buffer, attach-session; kill-server is test-only)
with the -F format strings as constants. Gates: `node tools/0_key_types.mjs` exits 0, `pnpm build` green.

## Contract suite
Build boop (`cargo build -p boop`), then from /Users/chrishafley/projects/boop2-mux:
`BOOP_BIN=<lane worktree>/target/debug/boop npm run test:contract` must be 0 fail (79 cases today).
HARD SAFETY: ~/.agent/boop.db holds irreplaceable user data; the suite's tripwire exits 98 if counts change: stop
and report. Never kill the default tmux server.

Commits end with `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. Do not push, do not
install boop over ~/.cargo/bin/boop. Report: the ownership query result, test counts, suite counts, both commit shas.
