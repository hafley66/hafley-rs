# Burndown item 6: boop-gc (issue `boop-gc`)

Implement plans/2026-09-30-boop-disk-plan.md steps 1-3 in v1 (read it fully):
1. One target root: `lane_target_root()` defaults to `~/.cache/boop/lanes` (where briefs already write);
   `lane create` REFUSES `--env CARGO_TARGET_DIR=<outside the root>` with a clear error (contract case 3 is skipped
   today for this: unskip it); the boop-start warmup cache (`justfile:113`, `~/.cache/boop/cargo-target`) moves to
   `<root>/_shared`.
2. `boop gc`: dry run by default, `--apply` deletes. One table: path, bytes, owner lane, lane state, reason.
   Reclaims: targets under the root whose lane is dead/retired/unregistered and untouched 24 h; trails of lanes dead
   7 days; merged worktrees (existing `lane delete` rule); nothing else. The disk floor calls the same code.
3. `lane list` hides dead coordinator rows older than 7 days; `gc --apply` deletes them.
The boop tsp gets the op: boop2 worktree `git -C /Users/chrishafley/projects/boop2 worktree add /Users/chrishafley/projects/boop2-gc -b burndown/gc main`
(symlink node_modules from /Users/chrishafley/projects/boop2-harmonize), `schema/lane/4_ops.tsp` + any model;
`node tools/0_key_types.mjs` 0, `pnpm build` green.

## HARD SAFETY
- gc deletes files. NEVER run `boop gc` (dry or apply) or any new boop binary against the user's real HOME, real
  ~/.agent, ~/.cache/boop or ~/.agent/boop.db. Only in the contract sandbox or Rust temp-dir tests.
- `gc` must never touch: ~/.agent/boop.db*, mail dirs, any path outside the lane target root/trail root/worktrees it
  owns, a live lane's anything. Tests prove each exclusion.
- Contract tripwire exit 98 = stop. No Python.

## Gates
- Contract cases 3 (refusal) and 11 (gc lists an unregistered target) unskipped and green; new cases: gc --apply
  removes only the listed dead target; a live lane's target is never listed; dead coordinator row expiry.
- `cargo test -p boop-proc -p boop --bin boop` green. CLAUDE.md rules (job.rs 6646 lines: new code in new numbered
  files). CARGO_BUILD_JOBS=4.
Commit both repos before reporting done; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
Do not push; do not install boop.
