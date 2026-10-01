# Burndown item 2: contract-prime-paths (issue `contract-prime-paths` in hafley-rs issues/)

Repo: boop2 (this lane's worktree, branch from main). Grow the black-box contract suite in `tests/` so the
user, lane, mail and route slices' CLI ops are covered. Read first: `tests/0_sandbox.bash`, existing `tests/*.bats`,
`tests/9_todo.txt` (uncovered ops, 112 today), `plans/0_e2e_contract.md`, `plans/1_core_interfaces.md`,
`schema/{user,lane,mail,route}/4_ops.tsp`.

Order: user slice first (tag add/list/rm/search/for/of/recent/sources/backfill, db favorite *, me mood/favorite),
then route (whoami, agent register, selection), mail (send, inbox, message, remind), lane (list/get/where/route/
revive/message/delete/retire).

Binary: `BOOP_BIN=/Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/writer-ds2/target/debug/boop`
(v1 with the four bug fixes). Run: `BOOP_BIN=... npm run test:contract`.

HARD SAFETY: ~/.agent/boop.db holds the user's irreplaceable favorites and tags. Every case goes through the
sandbox guard; `tests/run.sh` trips (exit 98) if real favorite/tag counts change during a run: stop and report.
Never kill the default tmux server.

Rules: snapshot style (existing helper), one `# covers:` line per case, `node tests/9_todo.mjs` count must drop;
cases v1 cannot pass get `lacks` skips with a reason (do not fake); comments max 2 consecutive lines; no Python;
no output in ~/.cache, ~/Library/Caches, /tmp.

Done when: suite green (0 fail), coverage count reported before/after, commits on the lane branch ending with
`Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. Do not push.
