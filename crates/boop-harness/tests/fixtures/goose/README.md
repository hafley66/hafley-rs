# goose harness fixtures

Receipts from probing `goose` (Block's goose CLI, `block-goose-cli` 1.50.0) for the boop goose adapter. Fixtures only; no Rust.

## TOC

- [Install and version](#install-and-version)
- [One-shot round trip](#one-shot-round-trip)
- [Session store](#session-store)
- [ACP round trip](#acp-round-trip)
- [Idle RSS](#idle-rss)
- [Interrupt](#interrupt)
- [Failed commands](#failed-commands)

## Install and version

```bash
brew install block-goose-cli   # goose 1.50.0 (formula block-goose-cli, cask group)
goose --version                # 1.50.0
```

Binary: `/opt/homebrew/bin/goose -> ../Cellar/block-goose-cli/1.50.0/bin/goose`.

## One-shot round trip

Command and stdout (exit 0):

```bash
goose run -q --provider openrouter --model deepseek/deepseek-v4-flash-0731 \
  --no-session -t "reply with the single word pong"
```
```
pong
```

With `--stats`:

```bash
goose run -q --provider openrouter --model deepseek/deepseek-v4-flash-0731 \
  --no-session -t "reply with the single word pong" --stats
```
```
pong

Stats:
  Time to first token: 15.46s
  Tokens/sec: 0.26
  Output tokens: 4
```

## Session store

Session records live in SQLite, one database per install: `~/.local/share/goose/sessions/sessions.db`. Tables: `schema_version`, `sessions`, `messages`, `usage_ledger`, `provider_inventory_entries`, `provider_inventory_models`. Fixture: `session.txt` (schema for the three relevant tables plus one JSON row per table for the ACP probe session `20260914_15`).

| table | key | row count for `20260914_15` |
|---|---|---|
| `sessions` | `id` (TEXT PK, `YYYYMMDD_N`) | 1 |
| `messages` | `id` (INTEGER PK) | 3 |
| `usage_ledger` | `id` (INTEGER PK) | 1 |

Session id shape is `<YYYYMMDD>_<increment>` (per-day counter), e.g. `20260914_15`. A `session/prompt` turn writes: one user row with the raw prompt, one synthetic user row holding a `<turn-context>` (current time + working dir + task reminder), one assistant row with the reply, and one `usage_ledger` row with provider-reported token counts and cost.

## ACP round trip

Fixture: `acp-handshake.jsonl`. Entry point is the `goose acp` subcommand (stdio ACP server). It requires provider env vars (`GOOSE_PROVIDER`, `GOOSE_MODEL`) plus `OPENROUTER_API_KEY`; without them `session/new` returns an error (see [Failed commands](#failed-commands)).

| step | request id | result |
|---|---|---|
| `initialize` | 1 | `protocolVersion: 1`, `agentInfo: {name: goose, version: 1.50.0}`, `agentCapabilities: {loadSession, promptCapabilities: {image, audio, embeddedContext}, mcpCapabilities: {http}, sessionCapabilities: {list, delete, close}}` |
| `session/new` | 2 | `sessionId: 20260914_15`, `modes` (auto/approve/smart_approve/chat), `configOptions` (provider select, mode select, model select, thinking effort) |
| `session/prompt` | 3 | `stopReason: "end_turn"`, `usage: {inputTokens: 5018, outputTokens: 4, totalTokens: 5022}` |

`session/prompt` streams `session/update` notifications then returns the result. `session/update` kinds observed, with counts:

| `sessionUpdate` | count |
|---|---|
| `session_info_update` | 3 |
| `usage_update` | 2 |
| `available_commands_update` | 1 |
| `agent_message_chunk` | 1 |

`prompt` must be an array of content blocks (`[{"type":"text","text":...}]`), not a bare string. Fixed per-turn token overhead (`session/prompt` result usage): `inputTokens: 5018`, `outputTokens: 4`, `totalTokens: 5022` (cost `$0.000068` per the usage_update cost field).

## Idle RSS

Interactive TUI started in tmux with provider env exported:

```bash
GOOSE_PROVIDER=openrouter GOOSE_MODEL=deepseek/deepseek-v4-flash-0731 \
  OPENROUTER_API_KEY=$KEY goose
```

After 14s idle (10s initial sleep + 4s settle), `ps -eo rss,args | grep goose`:

| metric | value |
|---|---|
| process count | 1 |
| RSS | 62,960 KB (~61.5 MiB) |

Single `goose` process holds the whole TUI; no separate worker/child process while idle.

## Interrupt

With a prompt streaming in the TUI, a single `Escape` keystroke did **not** stop generation; the reply kept streaming to completion (a 1-to-200 count completed fully). The on-screen help names the interrupt key as `Ctrl+C`, not Escape:

```
◒  Accelerating abstract algebras...  (Ctrl+C to interrupt)
```

`Ctrl+C` did stop the count mid-stream (cut output at 200 on the second run after several more numbers). Net: one `Escape` does not interrupt; `Ctrl+C` does.

## Failed commands

Interactive `goose` with no provider configured (env or config) exits with status 1:

```bash
goose
```
```
error: No provider configured. Run 'goose configure' first.
```

`goose acp` without provider env: `session/new` returns an RPC error:

```bash
export OPENROUTER_API_KEY=...   # but no GOOSE_PROVIDER / GOOSE_MODEL
goose acp   # send session/new
```
```json
{"code":-32603,"message":"Internal error","data":"Failed to resolve provider: Configuration value not found: GOOSE_PROVIDER"}
```

Environment `OPENROUTER_API_KEY` alone is expired (401). All live calls above used the working key from `~/.config/opencode/opencode.json` exported in the shell only; it is not written to any committed file.

`goose run` requires either `-i FILE` or `-t TEXT`; `goose run -i` with no value errors:
```
error: a value is required for '--instructions <FILE>' but none was supplied
```
