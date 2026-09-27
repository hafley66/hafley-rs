---
created: 2026-08-15
updated: 2026-08-15
type: task
status: done
priority: normal
epic: terminal-multiplexer-substrate
labels: [domain-boop, intent-research, artifact-contract]
lane: boop-mux-research
related: ['@boop-session-graph']
collision: [boop-mux-contract]
closed: 2026-08-15
---

# 018 Map tmux Herdr cmux and Cate runtime models

## Description

## Objective

Record how tmux, Herdr, cmux, and Cate own terminal processes, hierarchy, persistence, and control surfaces before extending `boop-mux`.

## Findings

### tmux

`Server -> Session -> WindowLink -> Window -> Pane -> PTY/process`, with clients attaching to sessions. A window may be linked into multiple sessions. Control uses one-shot CLI commands or the long-lived text control protocol.

### Herdr

Herdr implements its own Rust multiplexer rather than controlling tmux. Its server owns detachable terminal processes through `portable-pty`; clients attach over its socket protocol. The visible hierarchy is `named session/server namespace -> workspace -> tab -> layout tree -> pane`. Local source defines `Workspace`, `Tab`, `PaneRuntime`, `LayoutSnapshot`, typed pane/workspace/tab API schemas, persistence snapshots, and live handoff. Herdr can itself run inside tmux as an outer terminal.

Local evidence:

- `/Users/chrishafley/projects/ext/herdr/src/workspace.rs`
- `/Users/chrishafley/projects/ext/herdr/src/workspace/tab.rs`
- `/Users/chrishafley/projects/ext/herdr/src/layout.rs`
- `/Users/chrishafley/projects/ext/herdr/src/pane.rs`
- `/Users/chrishafley/projects/ext/herdr/src/api/schema/panes.rs`
- `/Users/chrishafley/projects/ext/herdr/src/persist/snapshot.rs`

### cmux

cmux is a native macOS terminal application in Swift/AppKit using libghostty. It owns GUI workspaces, splits, terminal surfaces, notification state, and a socket control API. tmux may run inside a cmux terminal. cmux restores layout and metadata and can resume supported agents from captured native resume tokens; arbitrary tmux, shell, and editor processes reopen as ordinary terminals after application restart.

Primary source: `https://github.com/manaflow-ai/cmux` and `https://cmux.com/docs/getting-started`.

### Cate

Cate is an Electron terminal host. A workspace owns panels; a terminal panel maps through `panelId` to a renderer terminal and a runtime `ptyId`; the process capability owns `Map<string, IPty>` from `node-pty`. Layout can be dock, canvas, or detached window placement. tmux is treated as a foreground program inside a Cate PTY. Agent presence code handles the detached tmux-server process topology, but Cate does not use tmux as its panel/session store.

Local evidence:

- `/Users/chrishafley/projects/cate-local/src/runtime/capabilities/process.ts`
- `/Users/chrishafley/projects/cate-local/src/renderer/lib/terminal/registryState.ts`
- `/Users/chrishafley/projects/cate-local/src/shared/types.ts`
- `/Users/chrishafley/projects/cate-local/src/runtime/capabilities/agentPresence.ts`

## Type consequence

Model two composable capabilities:

```rust
trait TerminalHost {
    fn terminals(&self) -> Result<Vec<Terminal>>;
    fn write(&self, terminal: &TerminalId, input: TerminalInput) -> Result<()>;
    fn resize(&self, terminal: &TerminalId, size: TerminalSize) -> Result<()>;
}

trait DetachableMultiplexer: TerminalHost {
    fn snapshot(&self) -> Result<MultiplexerSnapshot>;
    fn events(&self) -> Result<Box<dyn Iterator<Item = Result<MultiplexerEvent>>>>;
    fn attach(&self, target: &AttachTarget) -> Result<()>;
    fn detach(&self, client: &ClientId) -> Result<()>;
}
```

tmux and Herdr provide detachable multiplexer semantics. cmux and Cate provide terminal-host semantics and may contain tmux as a child program. Agent placement remains a relation over backend terminal coordinates.

## Acceptance Criteria

- [x] tmux ownership and window-link aliasing are recorded.
- [x] Herdr's local Rust ownership hierarchy and socket boundary are recorded.
- [x] cmux's native terminal-host and restore boundary are recorded.
- [x] Cate's panel, PTY, and tmux-process boundary are recorded.
- [x] Shared traits preserve terminal-host versus detachable-multiplexer capabilities.

## Tests Run

- [x] Local source searches across Herdr and Cate
- [x] Primary-source review for tmux, Herdr, and cmux
- [ ] `cargo test -p boop-mux` deferred because this task changes documentation only

## Implementation Notes

Research-only task. No runtime behavior changed.
