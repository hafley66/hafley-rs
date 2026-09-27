-- S184: Free-function names defined in three or more non-test, non-fixture files.
WITH RECURSIVE candidates AS MATERIALIZED (
  SELECT f._input_path, f.span__start, f.span__end, f.name
  FROM node AS f
  WHERE f.family = 'cst'
    AND f.kind = 'function_item'
    AND f.name IS NOT NULL
    AND instr('/' || replace(f._input_path, char(92), '/') || '/', '/tests/') = 0
    AND instr('/' || replace(f._input_path, char(92), '/') || '/', '/fixtures/') = 0
), ancestors(path, fn_start, fn_end, start, end, kind) AS (
  SELECT e._input_path, f.span__start, f.span__end,
         p.span__start, p.span__end, p.kind
  FROM candidates AS f
  JOIN edge AS e
    ON e._input_path = f._input_path
   AND e.family = 'cst'
   AND e.kind = 'child'
   AND e.to__start = f.span__start
   AND e.to__end = f.span__end
  JOIN node AS p
    ON p._input_path = e._input_path
   AND p.family = 'cst'
   AND p.span__start = e.from__start
   AND p.span__end = e.from__end
  UNION ALL
  SELECT a.path, a.fn_start, a.fn_end, p.span__start, p.span__end, p.kind
  FROM ancestors AS a
  JOIN edge AS e
    ON e._input_path = a.path
   AND e.family = 'cst'
   AND e.kind = 'child'
   AND e.to__start = a.start
   AND e.to__end = a.end
  JOIN node AS p
    ON p._input_path = e._input_path
   AND p.family = 'cst'
   AND p.span__start = e.from__start
   AND p.span__end = e.from__end
), free_functions AS (
  SELECT f._input_path, f.span__start, f.span__end, f.name
  FROM candidates AS f
  WHERE NOT EXISTS (
    SELECT 1 FROM ancestors AS a
    WHERE a.path = f._input_path
      AND a.fn_start = f.span__start
      AND a.fn_end = f.span__end
      AND a.kind IN ('impl_item', 'trait_item')
  )
), repeated_names AS (
  SELECT name
  FROM free_functions
  GROUP BY name
  HAVING COUNT(DISTINCT _input_path) >= 3
)
SELECT f._input_path, f.name, COUNT(*) AS definitions
FROM free_functions AS f
JOIN repeated_names AS r USING (name)
GROUP BY f._input_path, f.name
ORDER BY f._input_path, f.name;
