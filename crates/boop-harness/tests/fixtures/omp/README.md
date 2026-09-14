# omp harness fixtures

Receipts from probing `omp` (Oh My Pi) 18.1.21 for the boop omp adapter. Fixtures only; no Rust.

## TOC

- [Install and version](#install-and-version)
- [`--print` round trip](#print-round-trip)
- [Session file](#session-file)
- [ACP round trip](#acp-round-trip)
- [Failed commands](#failed-commands)

## Install and version

```bash
npm install -g @oh-my-pi/pi-coding-agent   # changed 155 packages in 10s
omp --version                              # omp/18.1.21
```

Binary: `/Users/chrishafley/.nvm/versions/node/v24.15.0/bin/omp`.

## `--print` round trip

Command and stdout (exit 0):

```bash
omp --print --allow-home --model deepseek/deepseek-v4-flash-0731 "reply with the single word pong"
```
```
Working...
pong
```

`--allow-home` keeps cwd at the worktree; without it omp auto-switches to a temp cwd (`/tmp`), see [Session file](#session-file).

## Session file

Fixture: `session.jsonl` (verbatim, interactive tmux run, no secrets).

tmux run:

```bash
tmux new-session -d -s omp-probe -c "$PWD" \
  "PI_CODING_AGENT_DIR=/tmp/omp-probe/agent omp --allow-home --model deepseek/deepseek-v4-flash-0731 'reply with the single word pong'"
# then Escape, /exit, tmux kill-session
```

Session directory encoding, from `PI_CODING_AGENT_DIR` root:

| cwd | session subdirectory |
|---|---|
| `/Users/chrishafley/projects/hafley-rs/.boop-worktrees/test/omp-harness-probe` | `-projects-hafley-rs-.boop-worktrees-test-omp-harness-probe` |
| `/Users/chrishafley/projects` | `-projects` |
| `/tmp` | `--private-tmp--` |
| `/var/tmp` | `--private-var-tmp--` |

Under `$HOME`: strip the `/Users/chrishafley` prefix, replace `/` with `-`. Outside `$HOME`: resolved `/private/...` path with separators as `-`, wrapped `--...--`.

File name shape: `<ISO8601-UTC, colons and dots as dashes>_<session-uuid>.jsonl`, e.g. `2026-09-14T18-21-03-191Z_01a0a126-b356-7000-9e69-cc5fadd7991d.jsonl`.

Line 1 (verbatim):

```json
{"type":"title","v":1,"title":"Reply with single word","source":"auto","updatedAt":"2026-09-14T18:21:34.726Z","pad":"                                                                        "}
```

`type` counts (`jq -r .type session.jsonl | sort | uniq -c`):

| type | count |
|---|---|
| custom | 1 |
| message | 2 |
| model_change | 1 |
| session | 1 |
| thinking_level_change | 1 |
| title | 1 |
| title_change | 1 |

Assistant message (`.type == "message" && .message.role == "assistant"`):

| key | value |
|---|---|
| `api` | `openrouter` |
| `provider` | `openrouter` |
| `model` | `deepseek/deepseek-v4-flash-0731` |
| `stopReason` | `stop` |
| `usage` keys | `input, output, cacheRead, cacheWrite, totalTokens, reasoningTokens, cost` |
| `usage.cost` keys | `input, output, cacheRead, cacheWrite, total` |
| other top-level keys | `role, content, api, provider, model, usage, stopReason, timestamp, responseId, providerPayload, duration, ttft, completedAt, contextSnapshot` |

## ACP round trip

Fixture: `acp-handshake.jsonl`. Entry point is the `omp acp` subcommand (`--mode` takes `text|json|rpc|rpc-ui`, not `acp`).

| step | request id | result |
|---|---|---|
| `initialize` | 1 | `protocolVersion: 1`, `agentInfo: {name: oh-my-pi, title: Oh My Pi, version: 18.1.21}`, `authMethods: [{id: agent, ...}]`, `agentCapabilities: {loadSession, mcpCapabilities, promptCapabilities, sessionCapabilities}` |
| `session/new` | 2 | `sessionId: 01a0a127-8e26-7000-a6a0-a4c1551df23a`, `configOptions` (mode select, model select) |
| `session/prompt` | 3 | `stopReason: "end_turn"`, `usage: {inputTokens: 16513, outputTokens: 5, totalTokens: 16518}` |

`session/prompt` returned `stopReason`. `session/update` notification kinds observed, with counts:

| `sessionUpdate` | count |
|---|---|
| `available_commands_update` | 1 |
| `session_info_update` | 2 |
| `agent_message_chunk` | 1 |
| `usage_update` | 1 |

## Failed commands

Environment `OPENROUTER_API_KEY` is expired (both `curl https://openrouter.ai/api/v1/key` and omp report `401 API key expired`). All live calls above used the working key in `~/.config/opencode/opencode.json`. Exact failure:

```bash
export OPENROUTER_API_KEY=...        # value from the environment
omp --print --model openrouter/deepseek/deepseek-v4-flash-0731 "reply with the single word pong"
```
```
Working...
401 API key expired.
```
exit 1.

`PI_CONFIG_DIR` is not honored as an omp state root. With `PI_CONFIG_DIR=$(mktemp -d)/omp-probe` exported, omp still wrote under `~/.omp/agent`. The working variable is `PI_CODING_AGENT_DIR` (help: "Session storage directory (default: ~/.omp/agent)").

```bash
PI_CONFIG_DIR=/tmp/x/omp-probe omp --print ... # session written to ~/.omp/agent/sessions/, not /tmp/x
PI_CODING_AGENT_DIR=/tmp/x/agent omp --print ... # session written to /tmp/x/agent/sessions/
```

`omp --mode acp` is not the ACP server. With stdin `/dev/null` it exited 0 with empty stdout. The ACP server is `omp acp`.
