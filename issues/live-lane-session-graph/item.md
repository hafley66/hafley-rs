---
created: 2026-08-17
updated: 2026-08-25
type: bug
status: open
priority: high
related: ['@boop-lane-observability']
labels: [domain-boop, intent-correctness]
---

# Live harness lane missing from session graph

## Description

Instant's Cmd+Shift agent panel invokes:

```text
boop agent sessions --format json
```

For a newly spawned OpenCode lane, Boop's canonical lane command and process
probe report a live route while the session graph emits no shell:

```text
boop beep lane list
live feature-capitalized-relation-names ... opencode ...

boop beep ps feature-capitalized-relation-names
feature-capitalized-relation-names  44233  28432  0.0  63  1

boop agent sessions --format json
{"schema_version":1,...,"shells":[]}
```

`boop beep lane get feature-capitalized-relation-names` reports the registered
route with `harness:"opencode"`, a tmux target, and `session_id:null` while the
harness conversation is still unresolved.

## Cause

`crates/boop/src/_0_session_graph.rs::load_agent_session_graph_with_runtime`
passes runtime rows through `shell_from_runtime`. That function returns `None`
when:

```rust
route.kind != "shell" && route.harness.is_some()
```

The lane is absent from the durable native-session set until a transcript
session resolves. Its registered route is simultaneously rejected from the
shell set because its kind is `lane` and its harness is `opencode`. The graph
therefore drops a live tmux-backed lane during this interval.

Instant consumes the documented typed graph and requires `shells` to contain
every live tmux-backed registered route that has not merged with a native
session node.

## Acceptance Criteria

- [x] A live registered `lane` route with a harness and unresolved session ID appears once in `AgentSessionGraph.shells`.
- [ ] The shell row carries lane, harness, mode, cwd, tmux, PID, and live state from the bounded runtime observation.
- [x] Once the matching native transcript session resolves, the graph merges or replaces the provisional shell without duplication.
- [x] Native session routes already represented in `sessions` remain absent from `shells`.
- [ ] Dead provisional routes remain excluded unless `--history` is requested.
- [ ] CWD filtering applies identically before and after native-session resolution.
- [ ] The public `boop agent sessions --format json` fixture covers unresolved, resolved, and dead transitions.
- [ ] Instant's external-shell projection fixture consumes the unresolved live lane as one shell.

## Tests Run

- [ ] cargo test -p boop session_graph
- [ ] cargo test -p boop --all-targets
- [ ] cargo clippy -p boop --all-targets -- -D warnings
- [ ] Instant focused Boop agent explorer tests

## Implementation Notes

The native-session membership check already runs before `shell_from_runtime`.
Use that evidence to suppress duplication. Do not classify route kind or
harness presence as proof that a native session row exists.

## Agent Runs

### 2026-08-25T18:52:10Z · @fix-native-visibility

Step 4: added unresolved->resolved lane test (unresolved_live_lane_appears_as_shell_then_merges_into_sessions) and fixed shell_from_runtime to drop a resolved kind=lane route whose session is in sessions. Ran: cargo test -p boop-store (133 passed). AC 8 (Instant external-shell projection fixture) out of scope.
