-- Run on query --scmpp's SQLite store with the fast database attached as facts.
-- Same input paths for both commands. Missing owners are explicit gap rows.
-- CST naming follows explicit function names, then binding/assignment/property names.
-- Render wrappers qualify only their direct function arguments; callbacks require
-- an enclosing component, hook, or wrapper. Namespace hooks require an uppercase identifier.
-- Class diagnostics remain outside this query's implemented rule set.
WITH
sources AS (
  SELECT DISTINCT p.id AS file, q.source__start AS start, q.source__text AS text
  FROM scmpp_row q JOIN scmpp_dict_path p ON p.text = q.path
),
cst AS (
  SELECT n.*, k.text AS kind_name, f.text AS field_name,
    CAST(substr(CAST(s.text AS BLOB), n.start - s.start + 1, n."end" - n.start) AS TEXT) AS text
  FROM scmpp_node n
  JOIN scmpp_dict_kind k ON k.id = n.kind
  LEFT JOIN scmpp_dict_field f ON f.id = n.field
  LEFT JOIN sources s ON s.file = n.file
),
frame_names AS (
  SELECT f.*,
    coalesce(
      (SELECT text FROM cst n WHERE n.file = f.file AND n.parent = f.pre AND n.field_name = 'name'),
      (SELECT n.text FROM cst parent JOIN cst n ON n.file = parent.file AND n.parent = parent.pre
        WHERE parent.file = f.file AND parent.pre = f.parent AND
          ((parent.kind_name = 'variable_declarator' AND f.field_name = 'value' AND n.field_name = 'name')
           OR (parent.kind_name = 'assignment_expression' AND f.field_name = 'right' AND n.field_name = 'left')
           OR (parent.kind_name = 'pair' AND f.field_name = 'value' AND n.field_name = 'key')
           OR (parent.kind_name = 'object_assignment_pattern' AND f.field_name = 'right' AND n.field_name = 'left')))
    ) AS function_name,
    EXISTS (SELECT 1 FROM cst args JOIN cst call ON call.file = args.file AND call.pre = args.parent
      JOIN cst callee ON callee.file = call.file AND callee.parent = call.pre AND callee.field_name = 'function'
      WHERE args.file = f.file AND args.pre = f.parent AND args.kind_name = 'arguments'
        AND call.kind_name = 'call_expression' AND callee.text IN ('memo', 'forwardRef', 'React.memo', 'React.forwardRef')) AS wrapper,
    EXISTS (SELECT 1 FROM cst parent WHERE parent.file = f.file AND parent.pre = f.parent
      AND parent.kind_name IN ('class_body', 'public_field_definition')) AS class_context
  FROM cst f WHERE f.kind_name IN
    ('function_declaration', 'function_expression', 'arrow_function',
     'generator_function_declaration', 'generator_function', 'method_definition')
),
frames AS (
  SELECT *, NOT class_context AND coalesce((wrapper OR
    (instr(function_name, '.') = 0 AND (function_name GLOB '[A-Z]*' OR function_name GLOB 'use[A-Z0-9]*')) OR
    (function_name GLOB '[A-Z]*.use[A-Z0-9]*')), 0) AS react
  FROM frame_names
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
  WHERE EXISTS (SELECT 1 FROM cst callee WHERE callee.file = n.file AND callee.parent = n.pre
    AND callee.field_name = 'function' AND (callee.kind_name = 'identifier' OR
      (callee.kind_name = 'member_expression' AND EXISTS (SELECT 1 FROM cst object
        WHERE object.file = callee.file AND object.parent = callee.pre AND object.field_name = 'object'
          AND object.kind_name = 'identifier' AND object.text GLOB '[A-Z]*'))))
),
hooks AS (
  SELECT h.*, coalesce(f.function_name, CASE WHEN f.wrapper THEN '<render>' END, h.raw_owner) AS owner,
    f.react, f.class_context, f.function_name,
    EXISTS (SELECT 1 FROM frames ancestor WHERE ancestor.file = h.file AND ancestor.pre < h.frame
      AND ancestor.last >= h.pre AND ancestor.react) AS inside_react
  FROM raw_hooks h LEFT JOIN frames f ON f.file = h.file AND f.pre = h.frame
),
eligible AS (
  SELECT * FROM hooks WHERE has_site AND raw_owner IS NOT NULL AND react
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
  FROM hooks WHERE has_site AND raw_owner IS NOT NULL AND NOT class_context
    AND function_name IS NOT NULL AND NOT react
  UNION
  SELECT path, start, "end", hook, owner, 'nested_callback',
    'anonymous callback inside a component, hook, or render wrapper'
  FROM hooks WHERE has_site AND raw_owner IS NOT NULL AND NOT class_context
    AND function_name IS NULL AND NOT react AND inside_react
  UNION
  SELECT path, start, "end", hook, owner, 'caller', 'hook at top level'
  FROM hooks WHERE has_site AND frame IS NULL
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
  FROM hooks WHERE frame IS NOT NULL AND raw_owner IS NULL
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
