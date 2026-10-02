---
created: 2026-10-02
updated: 2026-10-02
type: feature
reporter: claude-375
status: open
priority: normal
epic: burndown-2026-10
labels: [boop]
---

# boop: identify the caller from the process tree (no coordinator)

## Description

Today identity is --as then BOOP_SESSION (crates/boop-harness/src/identity.rs:99); unresolved exits 2. Add rungs: harness env (CLAUDE_CODE_SESSION_ID) -> ppid walk to a harness process -> claude ~/.claude/sessions/<pid>.json .sessionId; codex $CODEX_HOME/state_5.sqlite threads by cwd + created_at_ms (no pid column; ambiguity reported); omp unknown (~/.omp/agent/agent.db). Verified 2026-10-02: bash 74121 -> claude 99801, sessions/99801.json -> b9ad1897-... . Route = session id, registered on first use; lane parent comes from the same rung. boop2 C slice (lane c-first-slice) implements the same order.
