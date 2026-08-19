-- Named report: how many rows in a boop store belong to a test fixture lane.
-- Read-only. Run it before and after purge_fixture_lanes.sql.
--
--   sqlite3 -header -column ~/.agent/boop.db < crates/boop/sql/fixture_lanes.sql
--
-- `boop db "<sql>"` opens the store read-only (main.rs run_passthrough_at ->
-- ident::Store::open_readonly), so a purge cannot run through it; both files
-- here are plain sqlite3 scripts, visible and deletable.
--
-- The fixture set is the same list as purge_fixture_lanes.sql. Keep them equal.

CREATE TEMP VIEW fixture_lane AS
SELECT id, value FROM dict_session WHERE value IN (
  'mine', 'lane-test', 'lane-a', 'lane-x', 'test-lane', 'fake-lane',
  'some-lane', 'orphan-lane', 'durable-lane', 'sibling', 'chore-x');

SELECT 'agent_trace_event' AS "table",
       f.value AS lane,
       COUNT(*) AS n,
       MIN(datetime(e.created_ts / 1000, 'unixepoch')) AS first_utc,
       MAX(datetime(e.created_ts / 1000, 'unixepoch')) AS last_utc
  FROM agent_trace_event e
  JOIN fixture_lane f ON f.id = e.lane_id
 GROUP BY f.value

UNION ALL SELECT 'agent_trace', '*', COUNT(*), NULL, NULL
  FROM agent_trace WHERE root_session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_trace_span', '*', COUNT(*), NULL, NULL
  FROM agent_trace_span WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_lane', '*', COUNT(*), NULL, NULL
  FROM agent_lane
 WHERE lane_id IN (SELECT id FROM fixture_lane)
    OR parent_lane_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_edge', '*', COUNT(*), NULL, NULL
  FROM agent_edge
 WHERE parent_session_id IN (SELECT id FROM fixture_lane)
    OR child_session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_session', '*', COUNT(*), NULL, NULL
  FROM agent_session WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_turn', '*', COUNT(*), NULL, NULL
  FROM agent_turn WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_usage', '*', COUNT(*), NULL, NULL
  FROM agent_usage WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_live', '*', COUNT(*), NULL, NULL
  FROM agent_live WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'agent_live_span', '*', COUNT(*), NULL, NULL
  FROM agent_live_span WHERE session_id IN (SELECT id FROM fixture_lane)

UNION ALL SELECT 'dict_session', '*', COUNT(*), NULL, NULL FROM fixture_lane;
