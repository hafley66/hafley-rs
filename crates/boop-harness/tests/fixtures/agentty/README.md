# agentty harness fixtures

Receipts from probing `agentty` 0.8.0 (1ay1/agentty, C++26 static binary) for the boop agentty adapter. Fixtures only; no Rust.

## TOC

- [Install and version](#install-and-version)
- [One-shot `run` round trip](#one-shot-run-round-trip)
- [ACP round trip](#acp-round-trip)
- [Idle RSS](#idle-rss)
- [Interrupt](#interrupt)
- [Session on disk](#session-on-disk)
- [Failed commands](#failed-commands)

## Install and version

Static binary for macOS arm64, copied to a temp dir for the probe (no brew tap checked for a formula):

```bash
curl -fsSLO https://github.com/1ay1/agentty/releases/download/v0.8.0/agentty-macos-arm64
chmod +x agentty-macos-arm64
./agentty-macos-arm64 --version
```

```
agentty 0.8.0
log: /Users/chrishafley/.agentty/logs/agentty.log
```

`--version` also prints the log path; the binary writes state under `~/.agentty` (see [Session on disk](#session-on-disk)). `AGENTTY_HOME` overrides that root.

## One-shot `run` round trip

Headless one-shot is the `run [PROMPT]` subcommand (tools and sandbox run; the final answer prints to stdout). Exit 0:

```bash
agentty --provider openrouter -m deepseek/deepseek-v4-flash-0731 run "reply with the single word pong"
```

```
agentty: sandbox: unavailable, running unsandboxed (sandbox-exec missing)
Subagent report (general, 1 turn):

pong
```

The `sandbox-exec` missing line is emitted on stderr/stdout for every live run on this macOS host.

## ACP round trip

Fixture: `acp-handshake.jsonl`. Entry point is the `acp` subcommand (`agentty acp`). The full CLI is in `help.txt`; `acp --help` and `run --help` print the same top-level usage.

| step | request id | result |
|---|---|---|
| `initialize` | 1 | `protocolVersion: 1`, `agentInfo: {name: agentty, version: 0.8.0}`, `authMethods: []`, `agentCapabilities: {loadSession, mcpCapabilities, promptCapabilities, sessionCapabilities, auth, _meta.cancelRequest}` |
| `session/new` | 2 | `sessionId: 5e29faf05d0d07e1`, `modes.availableModes: [ask, write, minimal]` (default `ask`) |
| `session/prompt` | 3 | `stopReason: "end_turn"` |

`session/prompt` returns `stopReason` only; the usage figure is not in the result. `session/update` notification kinds observed, with counts:

| `sessionUpdate` | count |
|---|---|
| `available_commands_update` | 1 |
| `session_info_update` | 1 |
| `agent_message_chunk` | 1 |
| `usage_update` | 1 |

The `usage_update` notification carries the fixed per-turn overhead: `{"sessionUpdate":"usage_update","size":200000,"used":15526}`. The 15526 `used` tokens is the fixed context consumed for the one-word pong turn. `initialize` runs on first `session/new`; there is no separate startup cost beyond the prompt.

## Idle RSS

TUI started in tmux (`tmux new -d -s agentty-probe -c "$PWD" 'agentty --provider openrouter -m deepseek/deepseek-v4-flash-0731'`), 10 s idle, then summed RSS by args:

| process | RSS (KB) |
|---|---|
| `/tmp/agentty-probe/agentty-macos-arm64` | 103712 |

Single process, about 101 MB idle. A broad `ps -eo rss,args | grep -i agentty` also matched the harness spawn wrapper whose lane name contains `agentty-probe`; the number above is the actual binary only.

## Interrupt

With the TUI streaming a long poem, one `tmux send-keys -t agentty-probe Escape` stopped generation on the single press: the status line flipped from `streaming ... type to queue` to `type a message ...` and the turn ended mid-sentence. Interrupt on one ESC: yes. The `/exit` command then quit the TUI (pane exited 0).

## Session on disk

Sessions are JSON files (not a JSONL transcript), rooted at `$AGENTTY_HOME` (default `~/.agentty`). Directory tree:

| path | content |
|---|---|
| `~/.agentty/threads/index.json` | `{"threads":{"<hexid>":{"created_at","mtime","size","title","updated_at"}},"version":2}` |
| `~/.agentty/threads/<hexid>.json` | per-session record, see below |
| `~/.agentty/threads/acp_sessions.json` | `{<sessionId>: {cwd, title, updatedAt}}` (ACP only) |
| `~/.agentty/credentials/` | auth state (not inspected) |
| `~/.agentty/logs/agentty.log` | log file |

Session id is a 16-hex string (e.g. `5e29faf05d0d07e1`), also used as the ACP `sessionId`. The ACP pong session on disk:

```json
{
  "created_at": 1789426925,
  "id": "5e29faf05d0d07e1",
  "messages": [
    {"id": "2b9b47a4f8645952", "role": "user", "text": "reply with the single word pong\n", "timestamp": 1789426930, "tool_calls": []},
    {"id": "89d6333d26fa7081", "role": "assistant", "text": "pong", "timestamp": 1789426930, "tool_calls": []}
  ],
  "title": "reply with the single word pong ",
  "updated_at": 1789426925
}
```

Message shape: `{id, role, text, timestamp, tool_calls[]}`; no token usage persisted on the session record. Fixture: `session.json` (verbatim, no secrets).

## Failed commands

Without a key, the one-shot fails on authentication (exit 1):

```bash
agentty --provider openrouter -m deepseek/deepseek-v4-flash-0731 run "reply with the single word pong"
```

```
agentty: sandbox: unavailable, running unsandboxed (sandbox-exec missing)
Subagent report (general, 1 turn):

[subagent failed: not authenticated]

Activity:
◆ general agent
```
