---
created: 2026-08-22
updated: 2026-08-22
type: chore
status: closed
priority: normal
epic: harness-interface
related: ['@mail-over-doors']
labels: [domain-boop, intent-implementation]
size: M
---

# Delete tui/opencode/kimi channels and the codex InspectingProxy

## Description

Delete `boop-acp/src/channel/tui.rs` (865 lines), `channel/opencode.rs` (207),
`channel/kimi.rs` (174), and the proxy half of `channel/codex.rs` (605);
`boop tui codex` launches the TUI with `--remote` straight at the daemon and
reads the thread id from `state_5.sqlite`. Re-does `feature/acp-all-harnesses`
(`1fbc69e`) on the new trait; that branch conflicts with the `cli/` split and is
deleted after. Today's proxy fixes `1a100ee`, `b20c18c` are dead code here.
Lane P3.

## Acceptance Criteria

- [x] `LaneChannel` impls: `AcpChannel` plus test fakes only. `ClaudeChannel`
      and `CodexChannel` survive as the documented unwired rollback door and
      are constructed by nothing outside their own tests.
- [x] audit finding #12 closed
- [ ] `/resume` inside `boop tui codex` still works. The proxy is what used to
      hang the `/resume` picker (`1a100ee` accepted its extra sockets); with no
      proxy the picker talks to the daemon directly. Not exercised live: no
      codex TUI was launched, since the sibling sessions were working.

## Tests Run

`cargo build --workspace` clean.
`cargo clippy --workspace --all-targets` 0 warnings.
`cargo test --workspace --no-fail-fast` 656 passed, 0 failed, 8 ignored,
including the 7 reds this branch inherited.

Live, against a copy of `~/.agent/boop.db`:

| command | result |
|---|---|
| `boop beep lane list` | rc=0, 62 lines |
| `boop me` from `~/projects/sprefa` | rc=0, registered a codex route from `CODEX_THREAD_ID` |
| `boop tui codex` outside tmux | refuses on the `TMUX_PANE` check, before any launch |
| `cargo test -p boop-harness --lib door::codex` | 7 passed; the launch argv is `codex resume <thread> --remote unix://<socket> --cd <cwd>` |

## Implementation Notes

Landed as `c86832f`, `2ba1860`, `d2ff4c4`, `0de1a46` on
`refactor/harness-interface`. `crates/boop/docs/plan-harness-interface-2026-08-22.md`
§6 carries the row-by-row state, including the three `trait Harness` methods
kept and why.

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in
`src/**` (`tracing` only), no em dashes, banned identifiers.
