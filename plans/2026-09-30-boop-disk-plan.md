# boop: every byte a lane writes has one owner and one reclaim

## Where the bytes are (measured 2026-09-30)

| path | size | written by | reclaimed by |
| --- | --- | --- | --- |
| `~/.cache/boop/lanes/<name>/target` (29 dirs) | ~23 GB | briefs that set `CARGO_TARGET_DIR` by hand (`TASKS/the-gang-queue.RULES.md:28` style) | nothing: outside boop's target root, 20 of 29 names are not registered lanes |
| `~/.cache/boop/extract/<hash>` | 6.2 GB | unknown writer (no match in hafley-rs) | nothing |
| `~/.cache/boop/cargo-target` | deleted today, was gate size | `just boop-start` warmup (`justfile:113`) | nothing, by design ("keeps its own shared cache") |
| `~/.agent/lanes/<lane>` (1658 dirs) | 4.5 GB | boop: trails + lane targets (the real `lane_target_root`) | targets: supervisor exit and `lane delete`; trails: never |
| `~/.cache/lanes/<coordinator>/` | was work output | coordinators, by `CLAUDE.md` Delegation rule | nothing; deleted by hand today |
| `hafley-rs/.boop-worktrees` | 5.7 GB | `lane create` | `lane delete` when merged |
| `lane list` | 1600+ rows, most `dead ... REVIVABLE` from 400-570 h ago | coordinator registration | `lane prune` only when tmux is gone and pid dead |

## Causes

1. Three target locations. boop reclaims only its own root (`~/.agent/lanes`). The warmup cache and every hand-set `CARGO_TARGET_DIR` are outside it, and `lane create` lets a caller `--env CARGO_TARGET_DIR` win.
2. Lanes spawned outside `lane create` (the 20 unregistered names) have no row, so nothing knows to reclaim them.
3. Trails never expire.
4. Work output (bench DBs, clones, recon reports) was placed in cache dirs by rule, so cache cleanup deletes work.
5. Dead coordinator rows never expire, so `lane list` is unreadable.

## Fix, in order (each step is one commit, one test)

1. **One root.** `lane_target_root()` defaults to `~/.cache/boop/lanes` (the place everyone already writes). `lane create` refuses a `CARGO_TARGET_DIR` outside the root instead of letting it win. The warmup cache moves to `<root>/_shared` and is counted by the disk floor.
2. **`boop gc`** (dry run by default, `--apply` to delete). One table: path, bytes, owner lane, lane state, reason. Reclaims:
   - targets under the root whose lane is dead, retired, or unregistered and untouched for 24 h;
   - trails of lanes dead for 7 days;
   - merged worktrees (the existing `lane delete` rule);
   - `extract/` entries untouched for 7 days, once its writer is found and named in the table.
   The disk floor calls the same code, so there is one reclaim path.
3. **Expire rows.** `lane list` hides dead coordinator rows older than 7 days; `boop gc --apply` deletes them.
4. **Work output lives in the repo.** `CLAUDE.md` Delegation: recon reports go to `plans/recon/`; bench clones and DBs go to `crates/sprefa-extract/bench/` (clones and DBs gitignored, result tables committed as `plans/*.tsv`). `TASKS/the-gang-queue.RULES.md:28` drops its hand-set target line.
5. **Proof.** `boop gc` before and after on this machine; bytes reclaimed in a `plans/` tsv. Tests: a target outside the root is refused at create; an unregistered dir under the root is listed by gc; a live lane's target is never listed.

## Out of scope

Rebuild cost after reclaim (a reclaimed lane rebuilds on revive); cross-machine lanes.
