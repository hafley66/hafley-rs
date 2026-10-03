-- One portable SQL text per question over `ryii --resolve --kinds cst,call --sqlite`; sections split by 2_questions.sh.
-- @callers
SELECT DISTINCT caller_path, caller_name FROM resolved_edge
WHERE callee_path = 'crates/hafley_scm/src/span.rs' AND callee_name = 'node_text' AND caller_name IS NOT NULL
ORDER BY 1, 2;
-- @reaches
WITH RECURSIVE reach(path, name) AS (
  SELECT callee_path, callee_name FROM resolved_edge
  WHERE caller_path = 'crates/hafley_scm/src/lib.rs' AND caller_name = 'build' AND callee_name IS NOT NULL
  UNION
  SELECT e.callee_path, e.callee_name FROM reach AS r
  JOIN resolved_edge AS e ON e.caller_path = r.path AND e.caller_name = r.name
  WHERE e.callee_name IS NOT NULL
)
SELECT path, name FROM reach ORDER BY 1, 2;
-- @importers
SELECT DISTINCT src_path FROM resolved_import WHERE target_path = 'crates/hafley_scm/src/span.rs' ORDER BY 1;
-- @cycles
CREATE TEMP TABLE fdef AS
  SELECT _input_path AS path, span__start AS start, span__end AS stop, name FROM node
  WHERE family = 'call' AND kind IN ('function', 'method');
CREATE INDEX fdef_at ON fdef(path, start);
CREATE TEMP TABLE call AS
  SELECT DISTINCT
    (SELECT f.path || ':' || f.start || ':' || f.name FROM fdef f
      WHERE f.path = r.caller_path AND f.start <= r.caller_site_start AND r.caller_site_end <= f.stop
      ORDER BY f.start DESC LIMIT 1) AS caller,
    d.path || ':' || d.start || ':' || d.name AS callee
  FROM resolved_edge r JOIN fdef d ON d.path = r.callee_path AND d.start = r.callee_start;
CREATE INDEX call_from ON call(caller);
WITH RECURSIVE reach(origin, here) AS (
  SELECT caller, callee FROM call
  WHERE caller LIKE 'crates/hafley_scm/src/%' OR caller LIKE 'crates/sprefa-extract/src/%'
  UNION
  SELECT reach.origin, call.callee FROM reach JOIN call ON call.caller = reach.here
)
SELECT origin FROM reach WHERE origin = here ORDER BY 1;
