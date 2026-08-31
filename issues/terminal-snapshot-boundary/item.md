---
created: 2026-08-30
updated: 2026-08-30
type: task
status: open
priority: high
epic: terminal-multiplexer-substrate
related: ['@tmux-pane-projection', '@harness-interface', '@mux-paste-path', '@terminal-runtime-models']
labels: [domain-boop, domain-instant, component-terminal-host, intent-decoupling]
---

# Terminal snapshot boundary for host-neutral turn projection

## Description

Introduce a transport-neutral terminal snapshot boundary between terminal hosts and Boop turn projection. Harness transcript discovery and `(session, turn)` projection consume a current grid snapshot. tmux is one provider of lifecycle, input, and primary-screen history. A direct PTY terminal host is another provider.

This task changes seams and parity fixtures only. It does not replace tmux lifecycle, implement a terminal emulator, migrate the projection engine to Rust, or remove Instant's xterm renderer.

## Evidence

- `boop-harness` has no direct tmux process spawn or `Multiplexer` import. Its four harness adapters call tmux only in `Harness::spawn` and `Harness::stop`.
- Instant currently composes `XtermViewportAdapter` with `NativeTmuxPane` in `src/terminal.ts`; projection reads both.
- `portable-pty` is transport. `@xterm/xterm` is already present and provides direct-host terminal state and configurable retained rows.
- Current tmux path configures xterm `scrollback: 0`, delegates history to tmux, and exposes capture/copy-mode/paste through native tmux commands.
- Codex alternate-screen panes have no tmux history. Codex CLI supports `--no-alt-screen` for inline mode that preserves terminal scrollback.

## Type signatures

```rust
pub struct TerminalTarget { pub host: String, pub terminal: String, pub incarnation: u64 }
pub struct TerminalSize { pub columns: u16, pub rows: u16 }
pub enum Screen { Primary, Alternate }
pub enum History { Retained { rows: u32, capacity: u32 }, Unavailable }
pub struct TerminalRow { pub viewport_row: u16, pub text: String, pub wraps_previous: bool }
pub struct TerminalSnapshot { pub target: TerminalTarget, pub generation: u64, pub size: TerminalSize, pub screen: Screen, pub history: History, pub cursor: Option<(u16, u16)>, pub rows: Vec<TerminalRow> }
pub trait TerminalSnapshotSource: Send + Sync { fn snapshot(&self, target: &TerminalTarget) -> anyhow::Result<TerminalSnapshot>; }
```

## Instance timeline

1. A terminal host creates a terminal-state instance and assigns a target plus incarnation.
2. The host receives PTY bytes and updates its existing terminal-state library.
3. A projection request reads one immutable `TerminalSnapshot`.
4. The projection joins the snapshot with Boop turn rows and identifies turns by `(session, turn)`.
5. A resize, host replacement, active-screen change, or grid mutation causes a later snapshot generation.
6. Projection recomputes physical row spans from the current snapshot.
7. Final watcher cancellation stops observation. Terminal lifecycle remains owned by the terminal host.

## Storage and semantics

- The host owns PTY transport, terminal-state storage, scrollback capacity, input, resize, and terminal teardown.
- Boop projection retains only its prior immutable snapshot and turn-match result. It stores no byte stream or terminal-history copy.
- `TerminalTarget.incarnation` changes on terminal-state replacement.
- `generation` increases when rows, dimensions, active screen, cursor, or history facts change.
- `viewport_row` is local to a snapshot. It is never a stable identity.
- Width changes may reflow primary-screen history and visible rows. Projection recalculates from the new snapshot.
- Alternate screen exposes visible rows with `History::Unavailable`; it does not promise scrollback.
- tmux pane identifiers, sockets, session names, and history coordinates stay within the tmux adapter.

## Phase 1: seam and parity only

- Define the TypeScript and Rust-serializable terminal snapshot shape.
- Extract the current `XtermViewportAdapter` output into that shape.
- Extract `NativeTmuxPane` behind an optional host-observation adapter.
- Preserve current tmux capture/session lookup behavior behind the adapter.
- Add a direct-host fixture source using xterm-owned retained rows.
- Add harness launch presentation preferences sufficient to append Codex `--no-alt-screen` when a host advertises retained history.
- Keep tmux spawn, attach, detach, resize, copy mode, and paste APIs unchanged.
- Keep the existing xterm renderer and `portable-pty` transport unchanged.

## Non-goals

- No terminal emulator implementation.
- No broad runtime rewrite.
- No migration of Instant's turn projection to Rust.
- No removal of tmux lifecycle or user paste behavior.
- No stable absolute terminal-history coordinates across hosts.

## Acceptance Criteria

- [ ] Projection input is a `TerminalSnapshot` with snapshot-local viewport rows, dimensions, active screen, history availability, and generation.
- [ ] Harness transcript discovery and turn matching require no tmux type, pane id, session name, socket, or absolute history coordinate.
- [ ] Existing tmux capture/session lookup is an adapter implementation, not a projection input type.
- [ ] A direct PTY fixture backed by xterm retained rows produces the same visible-turn result as the equivalent tmux primary-screen fixture.
- [ ] Alternate-screen fixture reports unavailable history and projects only current visible rows.
- [ ] Resize/reflow fixture recomputes spans without preserving physical row ids across widths.
- [ ] Stable turn identity remains `(session, turn)` across a host snapshot refresh and resize.
- [ ] Codex launch-profile fixture appends `--no-alt-screen` only when the host advertises retained history.
- [ ] tmux launch-profile fixture preserves current tmux command behavior and Codex primary-screen behavior.
- [ ] Existing Instant terminal turn-visibility tests pass unchanged or are replaced by fixtures with byte-equivalent expected projections.
- [ ] `cargo test -p boop-harness -p boop-turnvis` and the selected Instant terminal test suite pass.

## Tests Run

Planning only.
