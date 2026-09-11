# Later ideas (2026-09-09)

Parking lot. Not scheduled. Recorded after the queue-e2e + interrupt plan session.

## TOC
1. Turn position reader
2. Rx-style call control
3. TUI turn replay against live harnesses

## 1. Turn position reader

Question it answers: "what turn am I on, and how far is the current call from the user message that started it?"

- Expose current turn number + call depth (distance from the turn's root user message) for a live session.
- Depth source exists in transcript parsing already: child/parent attribution reconstructs thread and turn ownership (`harness/codex.rs:674-843`, uncommitted incident fix).
- Surface through `boop` CLI read verbs and/or the native TUI projector (`cli/control.rs`).

## 2. Rx-style call control

Question it answers: "can I control harness calls like an rxjs stream?"

- Treat each harness call (prompt, tool, turn) as an observable: operators for filter, debounce, map, takeUntil.
- Natural fit points: `LaneChannel` events (`boop-acp/src/channel.rs`), door idle polling (`notify_idle`), supervisor loop (`supervise.rs`).
- Interrupt work (plan above) gives the stream a `takeUntil`/cancel primitive; this idea is the consumer side.
- Design constraint: the `signals` library (user's) is the reactive vocabulary to align with, not rxjs itself.

## 3. TUI turn replay against live harnesses

Question it answers: "can we prove door/queue pathing is correct without paying for real model calls?"

- Replay recorded TUI turns through the real door code paths (claude socket, codex app-server websocket, opencode HTTP) against recorded transcripts.
- Fixture convention already exists: `boop-harness/tests/fixtures/` with per-harness `sessions.json` + jsonl, `fixtures/CONVENTION.md`.
- Missing piece: a transport double per door (socket/ws/http server that answers from fixtures) so `door/claude.rs`, `door/codex.rs`, `door/opencode.rs` run for real against recorded wire shapes.
- Would give tier-3 coverage (real door queue admission, interrupt confirmation) with zero money calls and no tmux.
