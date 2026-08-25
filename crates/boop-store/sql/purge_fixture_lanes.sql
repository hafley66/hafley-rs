-- One-shot purge: delete every row a test fixture lane left in a boop store.
-- Destructive. Run fixture_lanes.sql first to see the counts, then:
--
--   sqlite3 ~/.agent/boop.db < crates/boop-store/sql/purge_fixture_lanes.sql
--
-- `boop db "<sql>"` opens the store read-only, so this runs through sqlite3
-- only. The fixture set is the same list as fixture_lanes.sql and
-- crates/boop/tests/temp_home_rail.rs FIXTURE_LANES. Keep the three equal.

CREATE TEMP VIEW fixture_lane AS
SELECT id, value FROM dict_session WHERE value IN (
  'mine', 'lane-test', 'lane-a', 'lane-x', 'test-lane', 'fake-lane',
  'some-lane', 'orphan-lane', 'durable-lane', 'sibling', 'chore-x');

BEGIN;
DELETE FROM agent_trace_event WHERE lane_id IN (SELECT id FROM fixture_lane)
   OR from_lane_id IN (SELECT id FROM fixture_lane)
   OR to_lane_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_trace_span WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_trace WHERE root_session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_lane WHERE lane_id IN (SELECT id FROM fixture_lane)
   OR parent_lane_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_edge WHERE parent_session_id IN (SELECT id FROM fixture_lane)
   OR child_session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_usage WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_turn WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_live_span WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_live WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_session WHERE session_id IN (SELECT id FROM fixture_lane);
DELETE FROM agent_mail WHERE from_route IN (SELECT value FROM fixture_lane)
   OR to_route IN (SELECT value FROM fixture_lane);
DELETE FROM agent_route WHERE route IN (SELECT value FROM fixture_lane);
COMMIT;

SELECT 'agent_trace_event left' AS what, COUNT(*) AS n FROM agent_trace_event
 WHERE lane_id IN (SELECT id FROM fixture_lane);
