---
created: 2026-08-19
updated: 2026-08-20
type: epic
owner: hafley66
status: open
priority: high
---

# boop-process: bash job control semantics + crate split (docs/design/boop-process.md)

## Description

## Description

Chris 2026-08-19: job control semantics from bash (`&`, `jobs`, `wait`, `kill`, `fg`, pipes, `trap`) as boop's one shape, and boop split into crates by responsibility. The analysis, the target verb table, the crate table, and the order of work are in `docs/design/boop-process.md`. Read that first; the cards below are its section 4.

## Cards

| # | card | size | blocked_by |
|---|---|---|---|
| 1 | boop-main-split (re-scoped: main.rs -> cli/*.rs by namespace, zero behavior change) | M | - |
| 2 | boop-crate-split (boop-store, boop-harness, boop-mail, boop-proc, boop-cli) | L | 1 |
| 3 | boop-job-namespace (`boop job`, `boop mail`, `boop me`; wait-all, kill vs rm, signal --children, attach, --timeout) | M | 2 |
| 4 | boop-mail-dir-global-flag + boop-hidden-verbs-retire | S | 3 |
| 5 | sprefa boop-hosted-in-dl6 (generated OpenAPI for /jobs /mail /me) | - | 3 |

## Comments

### 2026-08-20T15:47:10Z · @acp-all-harnesses-lane

ACP now drives every harness, not just opencode (design section 7: "scope widens from opencode to all harnesses after one is proven").

Flipped in 1fbc69e on branch feature/acp-all-harnesses:

- claude -> `npx -y @agentclientprotocol/claude-agent-acp`
- codex -> `npx -y @agentclientprotocol/codex-acp`
- kimi -> `kimi acp`
- opencode -> `opencode acp` (unchanged, now reading the same roster const)

`Harness::open_channel` mints an `AcpChannel` for all four; the adapter command per harness is a const in `crates/boop-acp/src/channel/acp.rs`. The old transports (claude stream-json, codex app-server, kimi tui) stay in the tree unwired as the rollback door.

Measured 2026-08-20, live end_turn on every adapter, plus a real claude lane turn through `boop beep lane run` (rc=0, result row hailed). All four advertise `loadSession`, so `--resume` rides `session/load`; live resume receipts for claude and kimi.

One open decision: boop spells a codex preset `gpt-5.6-luna@medium`, and codex-acp takes `gpt-5.6-luna` on the `model` config option with the effort on a separate `reasoning_effort` option. Nothing is translated, so such a lane fails at open with the offered ids in the message. Receipts in TASKS/acp-all-harnesses.REPORT.md section 4.
