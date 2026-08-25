# boop-start

The recipe every repo boop spawns into is expected to declare. A spawn runs it
in the fresh worktree before the harness starts, and tells the agent it ran.

| step | where |
|---|---|
| detect | `just --dump --dump-format json`, recipe `boop-start` (`worktree.rs` `find_start_recipe`) |
| run | `just boop-start` in the new worktree, killed at the 120s spawn deadline |
| record | `<mail-dir>/start/<lane>.status`, one line |
| tell | `lane run` puts that line plus the setup sentence ahead of the brief text |

Status line shapes, the second a notice the spawn continues past:

```
boop-start: ready in 15.4s (boop-start: cargo fetch and boop tests into ...)
boop-start: no recipe in /path/to/repo, nothing to warm
```

`boop beep lane create --dry-run` prints which of the two a real spawn would
take, and the justfile path the recipe comes from. `--no-start` skips the whole
step and says so.

A native has no injected first turn, so `boop beep agent register --worktree <dir>` warms that tree and prints the two lines to its own stdout.

The registration's last line tells the native how to name itself. A native
subagent runs inside its spawner's process, so no export can reach it and the
identity ladder's env rung keeps naming the spawner. Every verb the native runs
carries the name: `boop wait --me --as native-n1`, `boop beep <route> "<body>"
--as native-n1`, `boop beep parent "<body>" --as native-n1`. A bare `--me`
under a lane stamp that has live native children is refused with the
candidates listed (native-subagent-identity), never watched on the wrong
mailbox.

Recipe contract: idempotent, one summary line on stdout, under the spawn
deadline warm. This repo's builds into the shared target named by
`BOOP_CARGO_TARGET_DIR`, else `BOOP_START_CACHE`, else `~/.cache/boop`; 0.3s
warm, 15s cold.
