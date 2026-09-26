# One query per question: duplicate inventory (2026-09-26)

Scope: boop SQL (boop-store/boop-harness), boop CLI, instant Tauri commands, boop-xterm TS ports, boop-adapters.
instant links boop-store/boop-harness/boop-mux/boop-turnvis/boop-turnstrip by path (instant/src-tauri/Cargo.toml:47-51); it only shells out for `boop config presets`.

Paths: `ident.rs`/`query.rs`/`live.rs` = boop-store/boop-harness; `instant/` = ~/projects/instant/src-tauri/src; `xterm/` = hafley-rxjs/packages/boop-xterm/src.

| question | canonical | duplicate sites |
|---|---|---|
| Q1 turns for session | `query.rs:366` turn_rows_ordered | ident.rs:3497 query_turns, instant/0_boop.rs:211 turns_from (+MAX pre-query), 0_boop.rs:661 fork_reply, ident.rs:2063 last_assistant_turn, cli/db.rs, cli/screen.rs:46, cli/job.rs:1913, cli/me.rs:170/220, cli/control.rs:1056, cli/debug.rs, 0_harness_store.rs:19, xterm/6_turnVisibility.ts:35, instant/src/favorites.ts:127 (second TS cache) |
| Q2 recent turns across harness | `query.rs:516` recent_sessions | instant/0_boop.rs:251 (ascending, returns oldest 100), xterm/6_turnVisibility.ts:47, instant/src/favorites.ts:162/171, instant/1_boop_search.rs:134, boop-adapters/8_read.ts:16 |
| Q3 session in pane | `live.rs:220` session_in_pane_on_socket | instant/0_harness_store.rs:36, cli/me.rs:98, cli/control.rs:130, ident.rs:2965 (no callers), xterm/4_paneSession.ts:10 (1s/5s poll) |
| Q4 capture pane rows | `boop-mux/src/lib.rs:406` capture_pane (+pane_snapshot) | instant/1_squares.rs:145, instant/0_tmux.rs:9/57, cli/job.rs:3435, xterm/6_turnVisibility.ts:96, xterm/8f_forkRender.ts:52, xterm/2_turnLocate.ts input-row drop (TS copy of instant/0_boop.rs:986) |
| Q5 project turns onto rows | `instant/1_squares.rs:427` project_timed over `boop-turnvis/src/lib.rs:510` | instant/0_boop.rs:1036/1063 boop_locate_turns, boop/src/screen.rs:48 (`lane squares`, no reset boundary), boop-turnvis/_1_snapshot.rs:83, xterm/2_turnLocate.ts:128 (TS matcher), xterm/6_turnVisibility.ts:56/111 |
| Q6 sync transcript | `boop-harness/src/harness.rs:415` sync_session (+instant/0_boop.rs:142 candidate_for) | instant/0_boop.rs:160/301, cli/control.rs:1051, cli/db.rs:550, xterm/6_turnVisibility.ts:187 (debounced 120ms) |

## Spelling divergence for the same field
- row span: `buffer_start` (turnvis) / `bufferStart` (instant LocatedTurn, TS) / `viewport_start` (TurnSquare)
- confidence: `Anchored` (TurnSquare serde) / `anchored` (instant) / `pinned` (strip only)
- `session_scope`, `parent_session` snake inside camel instant wire
- `firstTurnTs` (boop-adapters) vs `ts`
- role `thinking` emitted only by instant (0a_boopPresentation.rs:41)

## Consumers of the Rust push today
squares_watch (instant/lib.rs:683 -> 1_squares.rs:481) reads Q1 turns_from + Q4 capture_lines + Q5 locate_visible_turns, session from Q3.
Readers: xterm/8j_agentSquares.ts:221,240; instant/src/1_agentSquares*.ts (duplicate client); instant/src/2_stripVisibility.ts:45.
The TS scan path (xterm/7_pane.ts:32 turnVisibilityStream) still runs boop_turns, boop_turns_recent, boop_mux_capture, boop_locate_turns, boop_sync_session.

Not opened by the inventory: boop_mux_session socket body, harness.rs:415 body, 1_ompTurnBinding.ts, boop_turn_annotations.
