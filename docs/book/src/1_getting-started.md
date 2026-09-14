# Getting started

## Prerequisites

- A Rust toolchain (the workspace builds on stable).
- `tmux` for the runtime paths that drive panes.
- [`just`](https://github.com/casey/just) for the recipes in the `justfile`.
- `cargo-nextest` for `just test`.
- `mdbook` 0.5.4 for building this book.

## Install the CLI

```bash
cargo install --path crates/boop --force
boop --help
```

`just install-boop` does the same from a clean tree on `origin/main` and stamps
the sha into `boop --version`.

## Test

```bash
just test          # cargo nextest run --workspace, the fast path
just test-ci       # cargo test --workspace --locked, exactly what CI runs
```

`just test` needs `cargo-nextest`; `cargo install cargo-nextest --locked`
installs it. CI runs `cargo test`, so a change has to pass `just test-ci`
before it ships.

## Spawn a lane

The branch is the whole identity: lane id and tmux session `fix-wait-boundary`,
worktree `.boop-worktrees/fix/wait-boundary`.

```bash
boop beep lane create --branch fix/wait-boundary --brief /abs/path/BRIEF.md \
  --preset flash4 --expect-commits-at-least 1 --dry-run
```

Drop `--dry-run` to spawn. The supervisor mails the parent on every turn end,
every commit, and on exit (`lane <id> done rc=<n>`). `--wait` blocks on that
row and exits with its rc.

## Send and wait

```bash
boop beep fix-wait-boundary "also run clippy"      # blocks for the reply
boop beep parent "done" --no-wait                  # the caller's own parent edge
boop wait fix-wait-boundary                        # the lane's result row
boop wait --me                                     # next unread mail to you
```

Exit codes: 0 reply or the recipient's turn ended, 3 route died, 4 lane exited
clean but an `--expect-*` assertion failed, 124 timeout.

## Read the store

```bash
boop db "SELECT * FROM agent_mail ORDER BY seq DESC LIMIT 20"
boop db "SELECT * FROM agent_delivery_transition ORDER BY sequence"
boop db status
BOOP_NO_SYNC=1 boop db "..."      # skip the startup transcript sync
```

Tables of note: `agent_mail`, `agent_route`, `agent_delivery_transition`,
`agent_lane`, `agent_trace`, `agent_trace_span`, `agent_turn`.
