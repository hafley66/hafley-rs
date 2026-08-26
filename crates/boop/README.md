# boop

One CLI for driving coding agents and reading what they did.

- Spawn an agent into its own git worktree and tmux pane (`beep lane create`).
- Mail any registered agent and block for its answer (`beep`, `wait`).
- Read every transcript on the machine (claude, codex, opencode, kimi) as rows
  in one SQLite store at `~/.agent/boop.db` (`db`).
- Ask what went wrong without opening a log (`debug`).

`boop --help` is the usage contract. Every flag below is in it.

## Install

```bash
cargo install --path crates/boop --force
```

## Verbs

| verb | job |
| --- | --- |
| `boop tui <harness>` | run an interactive harness in this pane and register it |
| `boop beep <route> <body>` | send, then block for the reply |
| `boop beep lane create` | worktree at base sha + spawn + route, one shot |
| `boop beep lane list/get/pane/patch/delete/prune` | the lane registry |
| `boop beep agent register/done` | pane-less routes: coordinators, native subagents |
| `boop beep ps [<lane>]` | pid, rss, cpu, uptime per live lane |
| `boop wait <id> \| <lane> \| --me` | block on a reply, a lane's exit, or your next unread mail |
| `boop debug [<lane>]` | recent WARN/ERROR grouped by lane; one lane in five sections |
| `boop db "<sql>"` | read-only SQL against the store |
| `boop whoami`, `boop config presets` | identity; model presets |

## Spawn a lane

```bash
boop beep lane create --branch fix/wait-boundary --brief /abs/path/BRIEF.md \
  --preset flash4 --expect-commits-at-least 1 --dry-run
```

The branch is the whole identity: lane id and tmux session `fix-wait-boundary`,
worktree `.boop-worktrees/fix/wait-boundary`. Drop `--dry-run` to spawn. The
supervisor mails the parent on every turn end, every commit and on exit
(`lane <id> done rc=<n>`). `--wait` blocks on that row and exits with its rc.

## Send and wait

```bash
boop beep fix-wait-boundary "also run clippy"      # blocks for the reply
boop beep parent "done" --no-wait                  # the caller's own parent edge
boop wait fix-wait-boundary                        # the lane's result row
boop wait --me                                     # next unread mail to you
```

Exit codes: 0 reply or the recipient's turn ended, 3 route died, 4 lane exited
clean but an `--expect-*` assertion failed, 124 timeout. The last line printed
is the next command to run.

## Read the store

```bash
boop db "SELECT * FROM agent_mail ORDER BY seq DESC LIMIT 20"
boop db "SELECT * FROM agent_delivery_transition ORDER BY sequence"
boop db status
BOOP_NO_SYNC=1 boop db "..."      # skip the startup transcript sync
```

Tables of note: `agent_mail`, `agent_route`, `agent_delivery_transition`,
`agent_lane`, `agent_trace`, `agent_trace_span`, `agent_turn`. Plain SQL only;
sqlite3 dot-commands are not supported.

## Identity

Two rungs: `--as <name>`, then the `BOOP_SESSION` env stamp that `boop tui`
writes. A session without either passes `BOOP_SESSION=<name>` on spawns.

## Presets

Model spelling is presets only. `boop config presets` prints name, harness,
model, effort, variant, bin and status, read from the platform config
directory's `boop/config.json`.

## License

MIT or Apache-2.0, at your option.
