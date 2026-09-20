---
created: 2026-09-15
updated: 2026-09-15
type: feature
status: open
priority: normal
---

# Human message visibility receipts and five-second coverage

## Description

# Human read receipts from visible messages and input inactivity

Status: proposed; requested 2026-09-15. Owner: boop-mux adapters, Boop storage/event system and Instant visibility projection.
Depends on stable message/turn identities. The existing visibility projection can support initial Instant work independently of the new navigator.

## Requested behavior
Tell agents which messages the human most recently had visible for more than one second while not typing. Correlate cmux window/workspace/pane/surface observations through the multiplexer abstraction with actual visible message ranges. Feed compact receipts into the future event/context system.

## Proposed signatures
```text
observe_attention(surface, focus, visible_ranges, human_input, monotonic_time) -> Observation
reduce_exposure(previous, observation) -> ExposureState + optional Receipt
recent_human_views(session, after_cursor) -> Receipts
```
Bodies: resolve surface to session; intersect message content with viewport; start or reset dwell based on focus, visibility and human input; emit a receipt after continuous eligible duration >1000 ms.

## Lifetime, storage and uniqueness
Track each observer/surface/message revision and visible range separately. A keypress, focus loss, hidden surface, scroll-out, unknown input capability or connection gap resets eligibility. Proposed default: a full continuous quiet interval after last human input. Use monotonic time for dwell and wall-clock timestamps for receipts. New streamed content needs its own exposure interval. Persist message/turn ID, session, surface identity, visible range, observed-at, dwell, method and confidence. Idempotency includes observer boot/event identity and exposure ID. Reconnect scans restore current state; elapsed disconnection time earns no dwell.

A receipt represents observed exposure, with partial content recorded explicitly. It does not establish comprehension. A last-seen turn number must not imply that every earlier message was seen. Long messages qualify only for the displayed ranges. Send agents compact factual summaries such as: message m42 lines 12-28 visible for 1.4 seconds without human typing.

## Evidence and adapter gap
- hafley-rs/crates/boop-mux/src/lib.rs: Multiplexer currently exposes pane/session lookup and capture, with Tmux implementation. No human-input/attention stream is declared.
- instant/src/0_terminalTurnVisibility.ts: visible, entered and exited turn projections with buffer regions.
- [cmux event contract](https://github.com/manaflow-ai/cmux/blob/main/docs/events.md), inspected 2026-09-15: window.keyed/unkeyed and workspace/surface/pane selection/focus are documented. surface.input_sent and surface.key_sent describe socket-originated input. They do not establish physical human typing coverage. A human-input signal and visible-range adapter require implementation or further source verification. notification.read records notification state and alone does not prove message exposure.

## Five-second coverage and three-second updates

User refinement: expose percentage of unique message content visible for at least five seconds, with navigator-square color reflecting relative exposure-time intensity. Publish time/coverage changes in three-second batches, with no one-second update stream. The earlier >1000 ms initial receipt remains a separate predicate.

Proposed coverage calculation: track eligible visibility by message revision and text offsets. Numerator is unique text whose eligible dwell reaches 5000 ms; denominator is known message text length. Repeated overlapping visibility does not double-count content. Proposed coverage dwell is cumulative across visits; continuous-versus-cumulative remains an open choice. The initial receipt requires continuous exposure. Typing/focus loss ends the current interval; unobserved time earns no credit. Streaming revisions update the denominator and range mapping explicitly. Empty/unmapped text shows unavailable coverage.

Account for local intervals on actual observation events with monotonic timestamps. Publish the latest changed aggregate per message every three seconds. Do not sample visibility only every three seconds or round dwell up to publication time. Store evidence independently of UI cadence. Label the UI metric as visible coverage; comprehension cannot be derived. Define the color scale, saturation cap and relative normalization scope before implementation.

- [ ] Coverage correctly handles overlap, revisit, reflow, streaming and missing mappings.
- [ ] Square color intensity follows a documented theme-compatible exposure scale.
- [ ] Time/coverage aggregates publish in three-second batches without a one-second stream.

## Decisions to resolve
Whether every visible pane or only focused pane counts; partial-range UI presentation; idle/absence timeout; receipt retention; opt-in setting; physical typing/IME event source. Proposed conservative default: focused visible surface, known human-input observation and range-specific receipts.

## Acceptance
- [ ] 1000 ms exactly does not pass; continuous duration greater than 1000 ms does.
- [ ] Typing, focus loss, scroll-out and disconnect reset the interval.
- [ ] Programmatic agent writes are distinguished from physical human typing.
- [ ] Partial and streaming messages preserve range/revision evidence.
- [ ] Unknown adapter capability creates no confirmed receipt.
- [ ] Replayed events and restarts create no duplicate or fabricated exposure.
- [ ] Agent context states observed exposure and timestamp without claiming comprehension.
