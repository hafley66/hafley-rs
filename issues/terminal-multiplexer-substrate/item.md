---
created: 2026-08-15
updated: 2026-08-15
type: epic
owner: chrishafley
status: deferred
priority: normal
labels: [domain-boop, component-multiplexer, intent-architecture]
---

# 017 Terminal multiplexer substrate

## Description

## Goal

Define the terminal-host and detachable-multiplexer relations Boop needs, bind tmux through `boop-mux`, and keep enough structure for compatible Herdr or native terminal-host adapters without copying their runtimes.

## Scope

- tmux server, client, session, window-link, window, pane, PTY, process, layout, and event relations
- typed query and control traits over those relations
- backend capability differences
- agent placement as a separate relation over multiplexer coordinates

## Acceptance Criteria

- [ ] Existing tmux calls are mapped to typed data and operation traits.
- [ ] Herdr, cmux, and Cate runtime models are recorded from primary sources or local code.
- [ ] Terminal hosts are distinguished from detachable multiplexer servers in the type model.
- [ ] Backend capability gaps are explicit.

## Tests Run

- [ ] `cargo test -p boop-mux`
- [ ] `issuectl doctor`

## Implementation Notes

Child work remains in flat Issuectl directories and references this epic through `epic: terminal-multiplexer-substrate`.
