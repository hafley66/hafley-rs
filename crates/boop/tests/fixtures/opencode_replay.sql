-- Scrubbed slice of opencode's real SQLite store shape, as read by
-- `Opencode::read_from`: `message(id, session_id, time_created, data-json)` and
-- the `part` rows its cursor requires. Committed as SQL text so no binary db
-- enters the tree; the test executes it with rusqlite before reading.
CREATE TABLE session (
  id TEXT PRIMARY KEY,
  directory TEXT,
  parent_id TEXT,
  slug TEXT,
  time_updated INTEGER
);
CREATE TABLE message (
  id TEXT PRIMARY KEY,
  session_id TEXT,
  time_created INTEGER,
  data TEXT
);
CREATE TABLE part (
  id TEXT,
  message_id TEXT,
  data TEXT
);
INSERT INTO session VALUES ('ses_replay_0001', '/Users/dev/replay', NULL, 'replay', 1767225602500);
INSERT INTO message VALUES ('msg_replay_0001', 'ses_replay_0001', 1767225600000, '{"role":"user"}');
INSERT INTO part VALUES ('part_replay_0001', 'msg_replay_0001', '{"type":"text","text":"run the replay"}');
INSERT INTO message VALUES ('msg_replay_0002', 'ses_replay_0001', 1767225601250, '{"role":"assistant","modelID":"opencode/replay-1","tokens":{"input":10,"output":4,"reasoning":1,"cache":{"read":2,"write":0}}}');
INSERT INTO part VALUES ('part_replay_0002', 'msg_replay_0002', '{"type":"text","text":"replayed"}');
INSERT INTO message VALUES ('msg_replay_0003', 'ses_replay_0001', 1767225602500, '{"role":"assistant","modelID":"opencode/replay-1","tokens":{"input":12,"output":5,"reasoning":0,"cache":{"read":1,"write":0}}}');
INSERT INTO part VALUES ('part_replay_0003', 'msg_replay_0003', '{"type":"text","text":"done"}');
