-- Run on query --scmpp's SQLite store with the fast database attached as facts.
-- Same input paths for both commands. Load bench_case(path, metadata) settings first.
-- Missing owners are explicit gap rows.
-- CST naming follows explicit function names, then binding/assignment/property names.
-- Render wrappers qualify only their direct function arguments; callbacks require
-- an enclosing component, hook, or wrapper. Namespace hooks require an uppercase identifier.
-- Generic DF owner metadata supplies async and direct class-method facts.
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
    (instr(function_name, '.') = 0 AND (function_name GLOB '[A-Z]*' OR (function_name = 'use' OR function_name GLOB 'use[A-Z0-9]*'))) OR
    (function_name GLOB '[A-Z]*.use[A-Z0-9]*')), 0) AS react
  FROM frame_names
),
raw_hooks AS (
  SELECT DISTINCT p.id AS file, q.path, q.hook__start AS start,
    q.hook__end AS "end", q.invoked__text AS hook, n.pre,
    (SELECT f.pre FROM frames f
      WHERE f.file = p.id AND f.pre < n.pre AND f.last >= n.pre
      ORDER BY f.depth DESC LIMIT 1) AS frame,
    d.function AS raw_owner, d.is_async, d.owner_kind,
    q.callee__text = 'use' AS is_use,
    EXISTS (SELECT 1 FROM facts.site s
      WHERE s._input_path = q.path AND s.family = 'call'
        AND s.span__start = q.invoked__start AND s.span__end = q.invoked__end) AS has_site
  FROM scmpp_row q JOIN scmpp_dict_path p ON p.text = q.path
  JOIN cst n ON n.file = p.id AND n.start = q.hook__start
    AND n."end" = q.hook__end AND n.kind_name = 'call_expression'
  LEFT JOIN facts.node d ON d._input_path = q.path AND d.family = 'df' AND d.kind = 'call_res'
    AND d.span__start = q.hook__start AND d.span__end = q.hook__end
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
reachable AS (
  -- A direct return makes later calls in its containing block unreachable.
  -- A return under an if does not make calls outside that branch unreachable.
  SELECT * FROM hooks h WHERE NOT EXISTS (
    SELECT 1 FROM returns r JOIN cst block ON block.file = r.file AND block.pre = r.parent
    WHERE r.file = h.file AND r.frame = h.frame AND r."end" <= h.start
      AND block.kind_name = 'statement_block' AND block.pre < h.pre AND block.last >= h.pre)
),
eligible AS (
  SELECT * FROM reachable WHERE has_site AND raw_owner IS NOT NULL AND react
),
ordered_hooks AS (SELECT * FROM eligible WHERE NOT is_use),
branches AS (
  SELECT h.path, h.start, h."end", h.hook, h.owner, a.pre
  FROM ordered_hooks h JOIN cst a ON a.file = h.file AND a.pre < h.pre
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

    -- A catch path skips calls reached after a potentially throwing call.
    OR (a.kind_name = 'try_statement' AND EXISTS (
      SELECT 1 FROM cst body JOIN cst previous ON previous.file = body.file
        AND previous.pre > body.pre AND previous.last <= body.last
      WHERE body.file = a.file AND body.parent = a.pre AND body.field_name = 'body'
        AND body.pre < h.pre AND body.last >= h.pre
        AND previous.kind_name IN ('call_expression', 'new_expression') AND previous."end" <= h.start
        AND NOT EXISTS (SELECT 1 FROM frames f WHERE f.file = h.file AND f.pre > h.frame
          AND f.pre < previous.pre AND f.last >= previous.pre))
      AND EXISTS (SELECT 1 FROM cst handler WHERE handler.file = a.file
        AND handler.parent = a.pre AND handler.kind_name = 'catch_clause'))
    -- A labeled break bypasses later calls in that label's block.
    OR (a.kind_name = 'labeled_statement' AND EXISTS (
      SELECT 1 FROM cst jump JOIN cst label ON label.file = jump.file AND label.parent = jump.pre
      JOIN cst target ON target.file = a.file AND target.parent = a.pre AND target.field_name = 'label'
      WHERE jump.file = a.file AND jump.kind_name = 'break_statement' AND jump.pre > a.pre
        AND jump.last <= a.last AND jump."end" <= h.start AND label.text = target.text
        AND NOT EXISTS (SELECT 1 FROM frames f WHERE f.file = h.file AND f.pre > h.frame
          AND f.pre < jump.pre AND f.last >= jump.pre)))
),
returns AS (
  SELECT r.*, (SELECT f.pre FROM frames f
    WHERE f.file = r.file AND f.pre < r.pre AND f.last >= r.pre
    ORDER BY f.depth DESC LIMIT 1) AS frame
  FROM cst r WHERE kind_name = 'return_statement'
),
looped AS (
  SELECT DISTINCT h.path, h.start, h."end", h.hook, h.owner
  FROM ordered_hooks h JOIN facts.df_nest nest ON nest._input_path = h.path
    AND nest.call__start = h.start AND nest.call__end = h."end"
  JOIN facts.df_loop loop ON loop._input_path = nest._input_path
    AND loop.span__start = nest.loop__start AND loop.span__end = nest.loop__end
  JOIN cst l ON l.file = h.file AND l.start = loop.span__start
    AND l."end" = loop.span__end AND l.pre > h.frame
    AND l.kind_name IN ('for_statement', 'for_in_statement', 'while_statement', 'do_statement')
),
scopes AS (
  SELECT * FROM cst WHERE kind_name IN ('program', 'statement_block', 'function_declaration',
    'function_expression', 'arrow_function', 'generator_function_declaration', 'generator_function',
    'method_definition', 'catch_clause')
),
bindings AS (
  SELECT name.*, declaration.pre AS declaration,
    (SELECT scope.pre FROM scopes scope WHERE scope.file = name.file
      AND scope.pre < name.pre AND scope.last >= name.pre
      ORDER BY scope.depth DESC LIMIT 1) AS scope
  FROM cst name JOIN cst declaration ON declaration.file = name.file AND declaration.pre = name.parent
  WHERE name.kind_name = 'identifier' AND
    ((declaration.kind_name = 'variable_declarator' AND name.field_name = 'name')
      OR (declaration.kind_name IN ('function_declaration', 'class_declaration') AND name.field_name = 'name')
      OR declaration.kind_name IN ('formal_parameters', 'required_parameter', 'optional_parameter')
      OR (declaration.kind_name = 'catch_clause' AND name.field_name = 'parameter'))
),
event_definitions AS (
  SELECT binding.*, owner.pre AS component, owner.function_name AS owner
  FROM bindings binding JOIN cst value ON value.file = binding.file
    AND value.parent = binding.declaration AND value.field_name = 'value' AND value.kind_name = 'call_expression'
  JOIN cst callee ON callee.file = value.file AND callee.parent = value.pre AND callee.field_name = 'function'
  JOIN frames owner ON owner.file = binding.file AND owner.pre = (
    SELECT frame.pre FROM frames frame WHERE frame.file = binding.file
      AND frame.pre < binding.pre AND frame.last >= binding.pre ORDER BY frame.depth DESC LIMIT 1)
  WHERE callee.text IN ('useEffectEvent', 'React.useEffectEvent') AND owner.react
),
event_references AS (
  SELECT reference.*, event.text AS event_name, event.owner, event.component,
    path.text AS path
  FROM event_definitions event JOIN scopes scope ON scope.file = event.file AND scope.pre = event.scope
  JOIN cst reference ON reference.file = event.file AND reference.text = event.text
    AND reference.kind_name IN ('identifier', 'shorthand_property_identifier')
    AND reference.pre > scope.pre AND reference.last <= scope.last AND reference.pre <> event.pre
  JOIN scmpp_dict_path path ON path.id = event.file
  WHERE NOT EXISTS (SELECT 1 FROM bindings shadow JOIN scopes shadow_scope
    ON shadow_scope.file = shadow.file AND shadow_scope.pre = shadow.scope
    WHERE shadow.file = reference.file AND shadow.text = reference.text AND shadow.pre <> event.pre
      AND shadow_scope.pre <= reference.pre AND shadow_scope.last >= reference.last
      AND shadow_scope.depth >= scope.depth)
    AND NOT EXISTS (SELECT 1 FROM bindings declaration WHERE declaration.file = reference.file
      AND declaration.pre = reference.pre)
),
event_violations AS (
  SELECT reference.* FROM event_references reference
  WHERE NOT EXISTS (
    SELECT 1 FROM frames callback JOIN cst args ON args.file = callback.file AND args.pre = callback.parent
    JOIN cst call ON call.file = args.file AND call.pre = args.parent AND call.kind_name = 'call_expression'
    JOIN cst callee ON callee.file = call.file AND callee.parent = call.pre AND callee.field_name = 'function'
    WHERE callback.file = reference.file AND callback.pre < reference.pre AND callback.last >= reference.last
      AND callback.pre > reference.component AND args.kind_name = 'arguments'
      AND (callee.text IN ('useEffect', 'React.useEffect', 'useLayoutEffect', 'React.useLayoutEffect',
        'useInsertionEffect', 'React.useInsertionEffect', 'useEffectEvent', 'React.useEffectEvent')
        OR EXISTS (SELECT 1 FROM bench_case config
          WHERE config.path = reference.path AND callee.kind_name = 'identifier'
            AND json_extract(config.metadata, '$.settings."react-hooks".additionalEffectHooks') IS NOT NULL
            AND regexp(json_extract(config.metadata, '$.settings."react-hooks".additionalEffectHooks'), callee.text))))
),
violations AS (
  SELECT path, start, "end", hook, owner, 'caller' AS rule,
    'owner name does not match component or hook convention' AS reason
  FROM reachable WHERE has_site AND raw_owner IS NOT NULL AND NOT class_context
    AND function_name IS NOT NULL AND NOT react
  UNION
  SELECT path, start, "end", hook, owner, 'nested_callback',
    'anonymous callback inside a component, hook, or render wrapper'
  FROM reachable WHERE has_site AND raw_owner IS NOT NULL AND NOT class_context
    AND function_name IS NULL AND NOT react AND inside_react AND NOT is_use
  UNION
  SELECT path, start, "end", hook, owner, 'caller', 'hook at top level'
  FROM reachable WHERE has_site AND frame IS NULL
  UNION
  SELECT path, start, "end", hook, owner, 'class',
    'hook belongs to a class method or class field function'
  FROM reachable WHERE has_site AND raw_owner IS NOT NULL
    AND (owner_kind = 'class_method' OR class_context)
  UNION
  SELECT path, start, "end", hook, owner, 'async',
    'owning component, hook, or render wrapper is async'
  FROM eligible WHERE is_async
  UNION
  SELECT path, start, "end", hook, owner, 'loop',
    'df_nest joins a loop within the owning function'
  FROM looped
  UNION
  SELECT path, start, "end", hook, owner, 'conditional',
    'hook is in a conditional branch within its function'
  FROM branches
  UNION
  SELECT h.path, h.start, h."end", h.hook, h.owner, 'early_return',
    'a return precedes this call within the same function'
  FROM ordered_hooks h JOIN returns r ON r.file = h.file AND r.frame = h.frame
    AND r."end" <= h.start
  WHERE NOT EXISTS (SELECT 1 FROM looped l WHERE l.path = h.path AND l.start = h.start)
  UNION
  SELECT h.path, h.start, h."end", h.hook, h.owner, 'try_catch',
    'use appears in a try body or catch within its owning function'
  FROM eligible h WHERE h.is_use AND EXISTS (
    SELECT 1 FROM cst ancestor WHERE ancestor.file = h.file AND ancestor.pre > h.frame
      AND ancestor.pre < h.pre AND ancestor.last >= h.pre
      AND (ancestor.kind_name = 'catch_clause' OR (ancestor.kind_name = 'statement_block'
        AND ancestor.field_name = 'body' AND EXISTS (SELECT 1 FROM cst parent
          WHERE parent.file = ancestor.file AND parent.pre = ancestor.parent AND parent.kind_name = 'try_statement'))))
  UNION
  SELECT path, start, "end", event_name, owner, 'effect_event',
    'effect event reference outside Effects and Effect Events in its component'
  FROM event_violations
  UNION
  SELECT path, start, "end", hook, owner, 'missing_owner',
    'df call_res owner absent for a call inside a function'
  FROM hooks WHERE frame IS NOT NULL AND raw_owner IS NULL
  UNION
  SELECT path, start, "end", hook, owner, 'missing_owner_metadata',
    'owning call lacks is_async or owner_kind metadata'
  FROM hooks WHERE frame IS NOT NULL AND raw_owner IS NOT NULL
    AND (is_async IS NULL OR owner_kind IS NULL)
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
  CASE WHEN rule IN ('missing_owner', 'missing_owner_metadata', 'missing_site', 'parse_error') THEN 'gap' ELSE 'violation' END AS status, reason
FROM violations ORDER BY path, start, rule;
