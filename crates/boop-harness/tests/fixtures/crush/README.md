# crush harness fixtures

Receipts from probing `crush` (Charm's crush, `charmbracelet/tap/crush` 0.94.2) for the boop harness. Fixtures only; no Rust. `crush` is a Go binary. It has no ACP entry point, so the ACP probe is a failure receipt.

## TOC

- [Install and version](#install-and-version)
- [One-shot round trip](#one-shot-round-trip)
- [Session store](#session-store)
- [ACP probe (unsupported)](#acp-probe-unsupported)
- [Idle RSS](#idle-rss)
- [Interrupt](#interrupt)
- [Failed commands](#failed-commands)

## Install and version

```bash
brew install charmbracelet/tap/crush   # crush 0.94.2
crush --version                        # crush version v0.94.2
```

Binary: `/opt/homebrew/bin/crush`. Formula `crush` 0.94.2 from the charmbracelet tap.

## One-shot round trip

Command and stdout (exit 0):

```bash
crush run --quiet --model openrouter/deepseek/deepseek-v4-flash-0731 "reply with the single word pong"
```
```
pong
```

Model flag accepts `provider/model`; `crush models` lists 275 openrouter models. `crush run` also reads the prompt from stdin or a file.

## Session store

Session records live in a per-project SQLite database at `<project>/.crush/crush.db` (data dir from `crush dirs`: `~/.config/crush` config, `<project>/.crush` data). The `<project>/.crush` directory is gitignored by crush (`.crush/.gitignore`). Fixture: `session.txt` (schema for `sessions` and `messages` plus one JSON row per table for the one-shot pong session `435d2f7f`).

| table | key | row count for `435d2f7f` |
|---|---|---|
| `sessions` | `id` (TEXT PK, UUID) | 1 |
| `messages` | `id` (TEXT PK, UUID) | 2 |

A one-shot `crush run` writes one `sessions` row (token counters + cost) and two `messages` rows (user prompt, assistant reply with `model`/`provider` on the assistant row). `parts` is a JSON array of `{type, data}` where the assistant row has `type: text` with `data.text` and a `type: finish` entry carrying `reason` (`end_turn`/`stop`).

## ACP probe (unsupported)

`crush` has no ACP server. There is no `crush acp` command, no `--acp` flag, and no `--mode acp` flag. The `crush server` command starts an HTTP/WebSocket server (`--host` TCP or Unix socket), not a stdio ACP server. Fixture `acp-handshake.jsonl` holds the sent `initialize` request only; no ACP response lines exist because the binary never enters an ACP protocol mode.

Piping an `initialize` request into `crush` crashes the binary:

```
  ERROR
  Crush crashed. If metrics are enabled, we were notified about it. If you'd like to report it, please copy the
  stacktrace above and open an issue at https://github.com/charmbracelet/crush/issues/new?template=bug.yml.
```

## Idle RSS

Interactive TUI started in tmux with `OPENROUTER_API_KEY` exported:

```bash
tmux new-session -d -s crush-probe -c "$PWD" "OPENROUTER_API_KEY=$KEY crush"
```

After 14s idle (10s sleep + settle, init prompt dismissed with Right+Enter), `ps -eo rss,args | grep crush`:

| metric | value |
|---|---|
| process count | 1 |
| RSS | 92,624 KB (~90.5 MiB) |

Single `crush` process holds the whole TUI; no separate child process while idle.

## Interrupt

With a long generation running in the TUI, a single `Escape` keystroke did **not** stop generation; the pane hint changes to `esc press again to cancel`. A second `Escape` cancels the run. The on-screen hint names Escape as a two-press cancel, not a one-press stop:

```
> Working!
esc press again to cancel • tab focus chat • / or ctrl+p commands ...
```

Net: one `Escape` does not interrupt; two `Escape` presses do. `Ctrl+C` quits the TUI.

## Failed commands

No ACP entry point exists:

```bash
crush acp
```
```
ERROR
Unknown command "acp" for "crush".
Try --help for usage.
```
```bash
crush --acp
```
```
ERROR
Unknown flag: --acp.
Try --help for usage.
```
```bash
crush --mode acp
```
```
ERROR
Unknown flag: --mode.
Try --help for usage.
```

`crush server` is an HTTP/WebSocket server, not a stdio ACP server; piping an `initialize` request to `crush server` never returns ACP JSON-RPC and hangs until timeout.

Environment `OPENROUTER_API_KEY` alone is expired (401). All live calls above used the working key from `~/.config/opencode/opencode.json` exported in the shell only; it is not written to any committed file.
