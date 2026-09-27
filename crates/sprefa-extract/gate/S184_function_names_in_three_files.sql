-- S184: Function names defined in three or more source files.
WITH repeated_names AS (
  SELECT name
  FROM node
  WHERE family = 'cst'
    AND kind = 'function_item'
    AND name IS NOT NULL
  GROUP BY name
  HAVING COUNT(DISTINCT _input_path) >= 3
)
SELECT node._input_path, node.name, COUNT(*) AS definitions
FROM node
JOIN repeated_names USING (name)
WHERE node.family = 'cst'
  AND node.kind = 'function_item'
GROUP BY node._input_path, node.name
ORDER BY node._input_path, node.name;
