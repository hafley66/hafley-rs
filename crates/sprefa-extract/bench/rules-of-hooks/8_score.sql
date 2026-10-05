-- pre-ryiii. One query, counts of messages matched by (case, rule, hook).
-- Repeated calls retain multiplicity; case_count is the non-Flow universe.
WITH rules(rule) AS (
  VALUES ('caller'), ('loop'), ('conditional'), ('early_return'), ('nested_callback')
  UNION SELECT DISTINCT rule FROM expected
  UNION SELECT DISTINCT rule FROM actual WHERE status = 'violation'
)
SELECT r.rule,
  (SELECT count(*) FROM bench_case WHERE syntax IS NULL OR syntax <> 'flow') AS case_count,
  coalesce(sum(min(m.expected_count, m.actual_count)), 0) AS true_positive,
  coalesce(sum(max(m.actual_count - m.expected_count, 0)), 0) AS false_positive,
  coalesce(sum(max(m.expected_count - m.actual_count, 0)), 0) AS false_negative
FROM rules r LEFT JOIN matched_counts m USING(rule)
GROUP BY r.rule ORDER BY r.rule;
