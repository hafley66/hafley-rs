## Description

## Description

Chris (2026-08-18): "can we set a mood in boop per node session where its effectively a data-attr on the session in the tree that says how i want the ai's to talk to each other, basically a log format".

A mood is one attribute row on a session node in the session graph. It names the format agents use when they message that session (hail bodies, lane completion mail, inbox drains). It cascades down the tree like a data attribute: a lane spawned from a coordinator inherits the coordinator's mood unless it sets its own.

## What exists

- per-session attributes today: `agent_favorite`, `agent_touch`, nickname on `agent_session`. No free-form attribute table.
- tree: `agent_lane.parent_lane_id`, `agent_edge`, `_0_session_graph.rs` focus rules.
- delivery: `boop hail`, `boop inbox drain --hook stop|prompt`, lane completion mail typed into the coordinator pane.

## Shape (planning protocol: signatures, storage, reads/writes, uniqueness)

- storage: `agent_session_attr(session_id INTEGER NOT NULL, key_id INTEGER NOT NULL, value TEXT NOT NULL, set_ts INTEGER NOT NULL, PRIMARY KEY (session_id, key_id)) WITHOUT ROWID`; keys in `dict_attr_key` (`mood` is the first key). Surrogate keys, dictionary for the natural key.
- write: `boop me mood <name>` (caller's session), `boop beep lane create --mood <name>` (child), `boop db "insert ..."` always works.
- read: `boop me` prints it; effective mood = first row walking `session -> parent_lane -> ... -> root`; hail/inbox format the body through the effective mood of the RECEIVER.
- moods are rows too: `mood(name, template)` in a table, seeded with `unga` (lists/tables/mermaid, no prose), `plain`, `board` (thread/state/waiting-on). No enum in code.

## Acceptance Criteria

- [x] `agent_session_attr` + `dict_attr_key` tables, migration, schema test.
- [x] `boop me mood <name>` writes; `boop me` shows the effective mood and which ancestor set it.
- [x] `boop beep lane create --mood` sets the child; a child with no mood resolves to the parent's.
- [x] hail / inbox drain / lane completion mail render through the receiver's effective mood; one test per delivery path with a fixture mood.
- [x] COUNT test: resolving the effective mood is one query (recursive CTE), not one per ancestor.
