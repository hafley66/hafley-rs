-- SQLite CLI, working directory = bench/rules-of-hooks, query store already open.
-- Retain the entire original case metadata in the same database as the query.
CREATE TABLE bench_case AS
SELECT json_extract(value, '$.id') AS case_id,
  json_extract(value, '$.file') AS path,
  json_extract(value, '$.validity') AS validity,
  json_extract(value, '$.syntax') AS syntax, value AS metadata
FROM json_each(CAST(readfile('1_cases.json') AS TEXT));

CREATE TABLE expected AS
WITH messages AS (
  SELECT c.case_id, c.path, CAST(e.key AS INTEGER) AS message_index,
    CASE e.type WHEN 'text' THEN e.value
      ELSE json_extract(e.value, '$.message') END AS message
  FROM bench_case c, json_each(c.metadata, '$.errors') e
  WHERE c.syntax IS NULL OR c.syntax <> 'flow'
), classified AS (
  SELECT *, CASE
    WHEN message LIKE '%early return%' THEN 'early_return'
    WHEN message LIKE '%called conditionally%' THEN 'conditional'
    WHEN message LIKE '%more than once%' THEN 'loop'
    WHEN message LIKE '%inside a callback%' THEN 'nested_callback'
    WHEN message LIKE '%class component%' THEN 'class'
    WHEN message LIKE '%async function%' THEN 'async'
    WHEN message LIKE '%try/catch%' THEN 'try_catch'
    WHEN message LIKE '%function created with React Hook%' THEN 'effect_event'
    WHEN message LIKE '%called in function%' OR message LIKE '%top level%' THEN 'caller'
    ELSE 'unclassified' END AS rule,
    CASE WHEN substr(message, 1, 1) = '`' THEN
      substr(message, 2, instr(substr(message, 2), '`') - 1)
    ELSE substr(message, instr(message, '"') + 1,
      instr(substr(message, instr(message, '"') + 1), '"') - 1) END AS hook
  FROM messages
)
SELECT * FROM classified;

CREATE VIEW matched_counts AS
WITH expected_counts AS (
  SELECT path, rule, hook, count(*) AS expected_count
  FROM expected GROUP BY path, rule, hook
), actual_counts AS (
  SELECT path, rule, hook, count(DISTINCT start) AS actual_count
  FROM actual WHERE status = 'violation' GROUP BY path, rule, hook
), keys AS (
  SELECT path, rule, hook FROM expected_counts
  UNION SELECT path, rule, hook FROM actual_counts
)
SELECT k.*, coalesce(e.expected_count, 0) AS expected_count,
  coalesce(a.actual_count, 0) AS actual_count
FROM keys k LEFT JOIN expected_counts e USING(path, rule, hook)
LEFT JOIN actual_counts a USING(path, rule, hook);

CREATE VIEW disagreements AS
SELECT m.path, m.rule, m.hook, m.expected_count, m.actual_count,
  CASE WHEN m.actual_count > m.expected_count THEN 'false_positive'
    ELSE 'false_negative' END AS disagreement,
  CASE
    WHEN m.actual_count > m.expected_count THEN coalesce(
      (SELECT group_concat(DISTINCT reason) FROM actual a
        WHERE a.path = m.path AND a.rule = m.rule AND a.hook = m.hook),
      'extra finding without a reference message')
    WHEN EXISTS (SELECT 1 FROM actual a WHERE a.path = m.path
      AND a.status = 'gap' AND (a.hook = m.hook OR a.hook IS NULL)) THEN
      (SELECT group_concat(DISTINCT reason) FROM actual a WHERE a.path = m.path
        AND a.status = 'gap' AND (a.hook = m.hook OR a.hook IS NULL)) ||
      CASE WHEN EXISTS (SELECT 1 FROM actual a WHERE a.path = m.path
        AND a.hook = m.hook AND a.status = 'violation' AND a.rule <> m.rule)
      THEN '; also reported under rules: ' || (SELECT group_concat(DISTINCT a.rule)
        FROM actual a WHERE a.path = m.path AND a.hook = m.hook
          AND a.status = 'violation' AND a.rule <> m.rule)
      ELSE '' END
    WHEN m.rule IN ('class', 'async', 'try_catch', 'effect_event') THEN
      'reference behavior outside the implemented rules'
    WHEN m.hook = 'use' OR m.hook LIKE '%.use' THEN
      'bare use does not match ^use[A-Z0-9]'
    WHEN EXISTS (SELECT 1 FROM actual a WHERE a.path = m.path
      AND a.hook = m.hook AND a.status = 'violation' AND a.rule <> m.rule) THEN
      'hook reported under rules: ' || (SELECT group_concat(DISTINCT a.rule)
        FROM actual a WHERE a.path = m.path AND a.hook = m.hook AND a.status = 'violation')
    WHEN NOT EXISTS (SELECT 1 FROM scmpp_row q
      WHERE q.path = m.path AND q.invoked__text = m.hook) THEN
      'no query capture for the reference hook name'
    ELSE 'call captured; no owning-function predicate matched this reference rule'
  END AS reason
FROM matched_counts m WHERE m.actual_count <> m.expected_count;
