# Worker messages delayed until coordinator completion

Observed 2026-09-10 in the Falcon coordinator thread
`01a07c48-4ca3-75c3-9fdb-17c4c4ca1c58`. Progress, result and completion messages
arrived as separate user turns after the work had already been reviewed and
deployed. Both source lanes were retired by then.

## Cause and change

`boop-harness/src/door/codex.rs::deliver` always used `codex queue`. The previous
incident fix accurately labeled queue admission but retained its next-turn
delivery behavior. The coordinator could not consume those messages during a
long-running turn.

The Codex door now reads only the newest turn's metadata using
`thread/turns/list` (`limit: 1`, descending, `itemsView: notLoaded`). For an
`inProgress` turn it calls `turn/steer` with the exact `expectedTurnId`. A
successful matching receipt returns `Delivered::Injected`; it does not also
enqueue the message. An idle turn or definitive RPC rejection falls back to the
existing queue. A missing/malformed steer acknowledgement returns an error and
does not send a second copy through the queue in that attempt. Existing mailbox
retry semantics remain separate; this change does not add transport-level
exactly-once delivery after an ambiguous connection loss.

The installed Codex 0.153.4 protocol was generated and checked locally. The
[official app-server documentation](https://learn.chatgpt.com/docs/app-server#steer-an-active-turn)
specifies active-turn steering and its turn-ID precondition. Full conversation
history is never loaded for delivery. Other harnesses and game code are unchanged.

## Verification and activation

Focused regression coverage: active progress bypasses the post-turn queue;
idle/explicit-rejection paths queue exactly once; lost or mismatched steer
receipts never trigger the second transport. Existing Codex socket and TUI
regressions are retained. A selected live-thread probe refuses queue fallback:

```sh
BOOP_CODEX_TEST_SOCKET=<selected-socket> \
BOOP_CODEX_TEST_THREAD=<selected-thread> \
BOOP_CODEX_TEST_MESSAGE='<explicit diagnostic text>' \
cargo test --locked -p boop-harness \
  door::codex::tests::active_turn_delivery_probe --lib -- --exact --ignored --nocapture
```

Omit `BOOP_CODEX_TEST_MESSAGE` to inspect queue counts/IDs without writing.
The live probe was accepted and its text appeared in this coordinator's active
turn, before completion. The old behavior would have deferred it to a later turn.

Full gate: `bash crates/boop/scripts/0_regression_gate.sh deterministic`.
Existing uncommitted native-completion fixes are preserved. Installation updates
future Boop invocations; resident wrappers already in memory require reopening.
This fix does not restart user sessions or delete queued/user messages.

Executed receipts:

- Focused Codex transport suite: 16 passed, zero failed; opt-in live probe run
  separately and passed. Initial sandbox run could not bind a fixture socket;
  the rerun with socket access passed.
- Full deterministic gate: 820 passed, zero failed, nine opt-in tests ignored;
  no-default-features check passed. Log: `/private/tmp/boop-delayed-messages-gate.log`.
- Current coordinator queue: zero submissions, no next page. No cleanup needed.
- Installed `boop 0.0.10 (b573718-dirty)` with locked/offline `cargo install`
  and two compile jobs. Log: `/private/tmp/boop-delayed-messages-install.log`.
- Installed CLI probe `m-9d627238` landed through `door`, and its text arrived
  during the active fix turn. It did not wait for final completion.
- Previous executable retained at `/private/tmp/boop-before-active-steer-b573718`.
