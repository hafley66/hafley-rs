# goose harness fixtures

Receipts from probing `goose` (Block) 1.50.0 for the boop goose adapter. Fixtures only; no Rust.

## TOC

- [Install and version](#install-and-version)
- [One-shot `run -t` round trip](#one-shot-run--t-round-trip)
- [ACP round trip](#acp-round-trip)
- [Idle RSS](#idle-rss)
- [Interrupt](#interrupt)
- [Session on disk](#session-on-disk)
- [Failed commands](#failed-commands)

## Install and version

```bash
brew install block-goose-cli   # pours block-goose-cli--1.50.0 (14 files, 328.9MB)
goose --version                #  1.50.0
```

Binary: `/opt/homebrew/bin/goose`.

Provider/backend via env (no interactive `goose configure` needed):

```bash
export GOOSE_PROVIDER=openrouter
export GOOSE_MODEL=deepseek/deepseek-v4-flash-0731
export OPENROUTER_API_KEY=...   # working key, exported for the lane only, never committed
```

Config dir `~/.config/goose`, session DB `~/.local/share/goose/sessions/sessions.db` (sqlite), logs `~/.local/state/goose/logs`.

## One-shot `run -t` round trip

Command and stdout (exit 0):

```bash
timeout 120 goose run -t 'reply with the single word pong' \
  --provider openrouter --model deepseek/deepseek-v4-flash-0731 --no-session -q
```

```
pong
```

`-q` suppresses the banner (`__( O)>` and `● new session · openrouter deepseek/deepseek-v4-flash-0731`); without it those lines precede the reply. `--no-session` avoids writing a session file; drop it to persist.

JSON mode adds usage metadata on stdout:

```bash
timeout 120 goose run -t 'reply with the single word pong' \
  --provider openrouter --model deepseek/deepseek-v4-flash-0731 --no-session -q --output-format json
```

| metadata | value |
|---|---|
| `total_tokens` | 4996 |
| `input_tokens` | 4992 |
| `output_tokens` | 4 |
| `cost_usd` | 0.0002862288 |
| `status` | `completed` |

## ACP round trip

Fixture: `acp-handshake.jsonl`. Entry point is `goose acp` (stdio server; `goose serve` is the HTTP/WebSocket variant).

| step | request id | result |
|---|---|---|
| `initialize` | 1 | `protocolVersion: 1`, `agentInfo: {name: goose, version: 1.50.0}`, `agentCapabilities: {loadSession, promptCapabilities{image,audio:false,embeddedContext}, mcpCapabilities{http:true,sse:false}, sessionCapabilities{list,delete,close}}`, `authMethods: [{id: goose-provider}]` |
| `session/new` | 2 | `sessionId: 20260914_9`, `modes: {auto, approve, smart_approve, chat}`, `configOptions` (provider, mode, model, thinking_effort), `_meta.workingDir` |
| `session/prompt` | 3 | `stopReason: "end_turn"`, `usage: {totalTokens: 4996, inputTokens: 4992, outputTokens: 4}` |

`session/update` notification kinds observed, with counts:

| `sessionUpdate` | count |
|---|---|
| `agent_message_chunk` | 1 |
| `available_commands_update` | 1 |
| `session_info_update` | 2 |
| `usage_update` | 2 |

`usage_update` carried `used: 4996, size: 1048576, cost: {amount: 1.686048e-05, currency: USD}`. Fixed token overhead per turn (system prompt + tools): input 4992 tokens.

## Idle RSS

Interactive TUI in tmux, 10s idle, then process scan:

```bash
tmux new -d -s goose-probe -c "$PWD" 'goose'
sleep 10
pgrep -x goose
```

| measure | value |
|---|---|
| goose processes | 1 |
| RSS | 20960 KB (20.5 MB) |

## Interrupt

Single `Escape` while the TUI was streaming a long story: generation did NOT stop. The key echoed into the output as a literal `^[` marker and streaming continued. A `C-c` stopped it (`> Interrupted, what should goose work on instead?`).

Single-ESC interrupt: NO. Goose requires `C-c` (a two-press interrupt is not single-ESC).

## Session on disk

Sessions are rows in the sqlite DB `~/.local/share/goose/sessions/sessions.db` (WAL mode), not per-session files. Fixture: `session.txt` (schema dump + one row per table as JSON).

Session id shape: `YYYYMMDD_N` (e.g. `20260914_9`), monotonic per day. Working dir stored per session. Tool call / turn history lives in the `messages` table; token usage in `usage_ledger`.

| table | row |
|---|---|
| `sessions` | id `20260914_9`, `session_type: user`, `working_dir: <worktree>`, `provider_name: openrouter`, `goose_mode: auto` |
| `messages` | user `reply with the single word pong`; assistant `pong` (`message_id` = `gen-<ts>-<suffix>`) |
| `usage_ledger` | model `deepseek/deepseek-v4-flash-0731`, input 4992, output 4, total 4996, cache_read 4864, cost 1.686e-05, `cost_source: provider_reported` |
| `provider_inventory_entries` | provider_id `openrouter`, provider_family `openrouter` |

## Failed commands

Environment `OPENROUTER_API_KEY` is expired (401 `API key expired`). All live calls above used the working key in `~/.config/opencode/opencode.json`, exported in-shell only. Exact failure:

```bash
timeout 120 goose run -t 'pong' --provider openrouter --model deepseek/deepseek-v4-flash-0731
```

```
error: Ran into this error: Authentication error: Authentication failed for https://openrouter.ai/api/v1/chat/completions. Status: 401 Unauthorized. Response: API key expired..
```

exit 1.

Bare interactive `goose` with no prior auth and no `GOOSE_*`/key env opens the OpenRouter OAuth browser flow (`Auth URL: https://openrouter.ai/auth?callback_url=...`) instead of using an API key; the env vars above are required to skip it.
