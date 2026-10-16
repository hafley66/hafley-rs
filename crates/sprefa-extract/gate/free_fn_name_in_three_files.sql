-- ledger: S186 free function names defined in 3+ non-test, non-fixture files
WITH RECURSIVE candidates AS MATERIALIZED (
  SELECT f._source, s.path, f.span__start, f.span__end, f.name
  FROM node AS f
  JOIN source AS s ON s.id = f._source
  WHERE f.family = 'cst'
    AND f.kind = 'function_item'
    AND f.name IS NOT NULL
    AND instr('/' || replace(s.path, char(92), '/') || '/', '/tests/') = 0
    AND instr('/' || replace(s.path, char(92), '/') || '/', '/fixtures/') = 0
), ancestors(source, fn_start, fn_end, start, end, kind) AS (
  SELECT e._source, f.span__start, f.span__end,
         p.span__start, p.span__end, p.kind
  FROM candidates AS f
  JOIN edge AS e
    ON e._source = f._source
   AND e.family = 'cst'
   AND e.kind = 'child'
   AND e.to__start = f.span__start
   AND e.to__end = f.span__end
  JOIN node AS p
    ON p._source = e._source
   AND p.family = 'cst'
   AND p.span__start = e.from__start
   AND p.span__end = e.from__end
  -- Equal-span CST nodes can revisit the same state. Deduplicate the visited
  -- (source, function span, ancestor span, kind) tuple to bound the walk.
  UNION
  SELECT a.source, a.fn_start, a.fn_end, p.span__start, p.span__end, p.kind
  FROM ancestors AS a
  JOIN edge AS e
    ON e._source = a.source
   AND e.family = 'cst'
   AND e.kind = 'child'
   AND e.to__start = a.start
   AND e.to__end = a.end
  JOIN node AS p
    ON p._source = e._source
   AND p.family = 'cst'
   AND p.span__start = e.from__start
   AND p.span__end = e.from__end
), free_functions AS (
  SELECT f._source, f.path, f.span__start, f.span__end, f.name
  FROM candidates AS f
  WHERE NOT EXISTS (
    SELECT 1 FROM ancestors AS a
    WHERE a.source = f._source
      AND a.fn_start = f.span__start
      AND a.fn_end = f.span__end
      AND a.kind IN ('impl_item', 'trait_item')
  )
), repeated_names AS (
  SELECT name
  FROM free_functions
  GROUP BY name
  HAVING COUNT(DISTINCT path) >= 3
)
SELECT f.path, f.name, f.span__start
FROM free_functions AS f
JOIN repeated_names AS r USING (name)
ORDER BY f.path, f.name, f.span__start;
