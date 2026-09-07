# Lane: opencode-model-switch-attribution (hafley-rs, boop-harness)

You work in `$PWD`, your own worktree of hafley-rs. Never `cd` elsewhere. Commit on your branch only.

## Symptom
In an opencode session the user switched the model mid-session (opencode's model picker). From that point boop's transcript sync lost turn attribution for that session: assistant turns are missing or land on the wrong role in `agent_turn` (store `~/.agent/boop.db`). Instant renders turns from those rows, so the pane's turn marks vanished.

## Where to look
- Adapter: `crates/boop-harness/src/harness/opencode.rs` (2063 lines). Discovery reads opencode's sqlite (`~/.local/share/opencode/opencode.db`, tables `session`, `message`, `part`, `session_message`) and projects rows into `agent_turn`. Projection version lives in `sync_cursor.projection_version` (`crates/boop-store`).
- Find the session: query opencode.db for sessions whose messages carry more than one distinct model or provider (the `message` row JSON has `modelID` and `providerID`; confirm the column names from the schema before writing SQL). Take the most recent such session in the last 7 days.
- Compare: `boop db "select turn, role, substr(body,1,60), ts from agent_turn where session_id='<id>' order by turn"` against the message rows. Locate the first message after the switch and state what the adapter did with it: skipped, merged into the previous turn, wrong role, or wrong session id.
- Read-only harness cost: `boop db` verbs and sqlite3 are read paths; do not run `boop db sync` against the live store. Copy `~/.agent/boop.db` to `$PWD/tmp/boop.db` if you need to run sync against a store.

## Deliverable
1. A written cause in `crates/boop/docs/2_opencode-model-switch.md`: session id, the message id where attribution broke, the adapter line that made the wrong call, and why (twenty lines maximum, a table for the message-to-turn comparison).
2. A fix in `opencode.rs` with a unit test that replays a minimal message sequence with a model switch through the projection and asserts every assistant turn lands with the right role and turn index. Follow the existing test style in that file.
3. `cargo test -p boop-harness --lib -- --test-threads=1` green. Paste the `test result` line into the commit body.

## Files you own (only these)
`crates/boop-harness/src/harness/opencode.rs`, `crates/boop/docs/2_opencode-model-switch.md`, and test fixtures you add under `crates/boop-harness/tests/fixtures/opencode/`.

## Style laws (code comments, commit message, doc)
- No em dashes. No sycophancy. No negative parallelism (`not X, Y`). No one-word sentences.
- Banned words: provenance, substrate, load-bearing, regime, grounded, ruling, honest, distill.
- No deictic filler (`here is`, `below`, `the following`).
- Commit subject exactly: `boop-harness: an opencode model switch keeps every later turn attributed`
