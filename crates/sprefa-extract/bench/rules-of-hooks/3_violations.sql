-- Run on query --scmpp's SQLite store with the fast database attached as facts.
-- Same input paths for both commands. Missing owners are explicit gap rows.
WITH
cst AS (
  SELECT n.*, k.text AS kind_name, f.text AS field_name
  FROM scmpp_node n
  JOIN scmpp_dict_kind k ON k.id = n.kind
  LEFT JOIN scmpp_dict_field f ON f.id = n.field
),
frames AS (
  SELECT * FROM cst WHERE kind_name IN
    ('function_declaration', 'function_expression', 'arrow_function',
     'generator_function_declaration', 'generator_function', 'method_definition')
),
raw_hooks AS (
  SELECT DISTINCT p.id AS file, q.path, q.hook__start AS start,
    q.hook__end AS "end", q.invoked__text AS hook, n.pre,
    (SELECT f.pre FROM frames f
      WHERE f.file = p.id AND f.pre < n.pre AND f.last >= n.pre
      ORDER BY f.depth DESC LIMIT 1) AS frame,
    (SELECT d.function FROM facts.node d
      WHERE d._input_path = q.path AND d.family = 'df' AND d.kind = 'call_res'
        AND d.span__start = q.hook__start AND d.span__end = q.hook__end
      LIMIT 1) AS raw_owner,
    EXISTS (SELECT 1 FROM facts.site s
      WHERE s._input_path = q.path AND s.family = 'call'
        AND s.span__start = q.invoked__start AND s.span__end = q.invoked__end) AS has_site
  FROM scmpp_row q JOIN scmpp_dict_path p ON p.text = q.path
  JOIN cst n ON n.file = p.id AND n.start = q.hook__start
    AND n."end" = q.hook__end AND n.kind_name = 'call_expression'
),
hooks AS (
  SELECT h.file, h.path, h.start, h."end", h.hook, h.pre, h.frame, h.has_site,
    CASE WHEN h.raw_owner GLOB '*::closure::*' OR h.raw_owner GLOB 'closure@*'
      THEN coalesce((SELECT d.name FROM facts.node d JOIN frames f
        ON f.file = h.file AND f.pre = h.frame
        WHERE d._input_path = h.path AND d.family = 'call' AND d.kind = 'lambda'
          AND d.span__start = f.start AND d.span__end = f."end"
        LIMIT 1), h.raw_owner)
      ELSE h.raw_owner END AS owner
  FROM raw_hooks h
),
eligible AS (
  SELECT * FROM hooks WHERE has_site AND
    (owner GLOB '[A-Z]*' OR owner GLOB 'use[A-Z0-9]*')
    AND owner NOT GLOB '*::closure::*' AND owner NOT GLOB 'closure@*'
),
branches AS (
  SELECT h.path, h.start, h."end", h.hook, h.owner, a.pre
  FROM eligible h JOIN cst a ON a.file = h.file AND a.pre < h.pre
    AND a.last >= h.pre AND a.pre > h.frame
  WHERE
    (a.kind_name IN ('if_statement', 'ternary_expression') AND EXISTS (
      SELECT 1 FROM cst arm WHERE arm.file = a.file AND arm.parent = a.pre
        AND arm.field_name IN ('consequence', 'alternative')
        AND arm.pre <= h.pre AND arm.last >= h.pre))
    OR (a.kind_name = 'binary_expression' AND EXISTS (
      SELECT 1 FROM cst op WHERE op.file = a.file AND op.parent = a.pre
        AND op.kind_name IN ('&&', '||', '??')) AND EXISTS (
      SELECT 1 FROM cst rhs WHERE rhs.file = a.file AND rhs.parent = a.pre
        AND rhs.field_name = 'right' AND rhs.pre <= h.pre AND rhs.last >= h.pre))
),
returns AS (
  SELECT r.*, (SELECT f.pre FROM frames f
    WHERE f.file = r.file AND f.pre < r.pre AND f.last >= r.pre
    ORDER BY f.depth DESC LIMIT 1) AS frame
  FROM cst r WHERE kind_name = 'return_statement'
),
violations AS (
  SELECT path, start, "end", hook, owner, 'caller' AS rule,
    'owner name does not match component or hook convention' AS reason
  FROM hooks WHERE has_site AND owner IS NOT NULL AND owner NOT GLOB '[A-Z]*'
    AND owner NOT GLOB 'use[A-Z0-9]*' AND owner NOT GLOB 'closure@*'
    AND owner NOT GLOB '*::closure::*'
  UNION
  SELECT path, start, "end", hook, owner, 'nested_callback',
    'innermost owner is an anonymous closure'
  FROM hooks WHERE has_site AND (owner GLOB 'closure@*' OR owner GLOB '*::closure::*')
  UNION
  SELECT path, start, "end", hook, owner, 'caller', 'hook at top level'
  FROM hooks WHERE has_site AND frame IS NULL AND owner IS NULL
  UNION
  SELECT h.path, h.start, h."end", h.hook, h.owner, 'loop',
    'df_nest joins a loop within the owning function'
  FROM eligible h JOIN facts.df_nest nest ON nest._input_path = h.path
    AND nest.call__start = h.start AND nest.call__end = h."end"
  JOIN facts.df_loop loop ON loop._input_path = nest._input_path
    AND loop.span__start = nest.loop__start AND loop.span__end = nest.loop__end
  JOIN cst l ON l.file = h.file AND l.start = loop.span__start
    AND l."end" = loop.span__end AND l.pre > h.frame
    AND l.kind_name IN ('for_statement', 'for_in_statement', 'while_statement', 'do_statement')
  UNION
  SELECT path, start, "end", hook, owner, 'conditional',
    'hook is in a conditional branch within its function'
  FROM branches
  UNION
  SELECT h.path, h.start, h."end", h.hook, h.owner, 'early_return',
    'a return precedes this call within the same function'
  FROM eligible h JOIN returns r ON r.file = h.file AND r.frame = h.frame
    AND r."end" <= h.start
  UNION
  SELECT path, start, "end", hook, owner, 'missing_owner',
    'df call_res owner absent for a call inside a function'
  FROM hooks WHERE frame IS NOT NULL AND owner IS NULL
  UNION
  SELECT path, start, "end", hook, owner, 'missing_site',
    'query hook has no matching fast call site'
  FROM hooks WHERE NOT has_site
  UNION
  SELECT p.text, n.start, n."end", NULL, NULL, 'parse_error',
    'tree-sitter ERROR node in this input'
  FROM cst n JOIN scmpp_dict_path p ON p.id = n.file WHERE n.kind_name = 'ERROR'
)
SELECT path, start, "end", hook, owner, rule,
  CASE WHEN rule IN ('missing_owner', 'missing_site', 'parse_error') THEN 'gap' ELSE 'violation' END AS status, reason
FROM violations ORDER BY path, start, rule;
