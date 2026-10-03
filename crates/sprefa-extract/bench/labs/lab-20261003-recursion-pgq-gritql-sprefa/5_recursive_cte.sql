-- @T1
WITH RECURSIVE
call_kind AS (SELECT id FROM scmpp_dict_kind WHERE text = 'call_expression'),
spine AS (SELECT id FROM scmpp_dict_field WHERE text IN ('function', 'value')),
walk(file, origin, outer_start, outer_end, pre) AS (
  SELECT n.file, n.pre, n.start, n."end", n.pre FROM scmpp_node AS n WHERE n.kind = (SELECT id FROM call_kind)
  UNION ALL
  SELECT w.file, w.origin, w.outer_start, w.outer_end, c.pre FROM walk AS w
  JOIN scmpp_node AS c ON c.file = w.file AND c.parent = w.pre
  WHERE c.field IN (SELECT id FROM spine)
),
link AS MATERIALIZED (
  SELECT w.file, w.origin, w.outer_start, w.outer_end, w.pre, k.start AS link_start, k."end" AS link_end
  FROM walk AS w JOIN scmpp_node AS k ON k.file = w.file AND k.pre = w.pre
  WHERE w.pre <> w.origin AND k.kind = (SELECT id FROM call_kind)
)
SELECT p.text AS path, x.outer_start, x.outer_end, x.link_start, x.link_end
FROM link AS x JOIN scmpp_dict_path AS p ON p.id = x.file
WHERE NOT EXISTS (SELECT 1 FROM link AS y WHERE y.file = x.file AND y.pre = x.origin);
-- @T3
WITH RECURSIVE reach(anchor, fn_id) AS (
  SELECT s.anchor, e.dst_fn_id FROM seed AS s JOIN call_edge_{tier} AS e ON e.src_fn_id = s.fn_id WHERE e.extern = 0
  UNION
  SELECT r.anchor, e.dst_fn_id FROM reach AS r JOIN call_edge_{tier} AS e ON e.src_fn_id = r.fn_id WHERE e.extern = 0
)
SELECT r.anchor, p.text AS path, n.text AS name FROM reach AS r JOIN fn AS f ON f.id = r.fn_id
JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id;
-- @T4
WITH RECURSIVE
bound AS (SELECT count(*) AS hops FROM fn),
walk(anchor, target, fn_id, depth) AS (
  SELECT t.anchor, t.target_fn_id, t.seed_fn_id, 0 FROM t4_pair AS t WHERE t.tier = '{tier}'
  UNION
  SELECT w.anchor, w.target, e.dst_fn_id, w.depth + 1 FROM walk AS w JOIN call_edge_{tier} AS e ON e.src_fn_id = w.fn_id
  WHERE e.extern = 0 AND w.fn_id <> w.target AND w.depth < (SELECT hops FROM bound)
)
SELECT w.anchor, p.text AS target_path, n.text AS target_name, min(w.depth) AS length
FROM walk AS w JOIN fn AS f ON f.id = w.target JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id
WHERE w.fn_id = w.target AND w.depth > 0 GROUP BY 1, 2, 3;
-- @T3_key
WITH RECURSIVE reach(anchor, fn_id) USING KEY (anchor, fn_id) AS (
  SELECT s.anchor, e.dst_fn_id FROM seed AS s JOIN call_edge_{tier} AS e ON e.src_fn_id = s.fn_id WHERE e.extern = 0
  UNION ALL
  SELECT r.anchor, e.dst_fn_id FROM reach AS r JOIN call_edge_{tier} AS e ON e.src_fn_id = r.fn_id
  WHERE e.extern = 0 AND NOT EXISTS (SELECT 1 FROM recurring.reach AS k WHERE k.anchor = r.anchor AND k.fn_id = e.dst_fn_id)
)
SELECT r.anchor, p.text AS path, n.text AS name FROM reach AS r JOIN fn AS f ON f.id = r.fn_id
JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id;
-- @T4_key
WITH RECURSIVE bfs(anchor, target, fn_id, depth) USING KEY (anchor, fn_id) AS (
  SELECT t.anchor, t.target_fn_id, t.seed_fn_id, 0 FROM t4_pair AS t WHERE t.tier = '{tier}'
  UNION ALL
  SELECT b.anchor, b.target, e.dst_fn_id, b.depth + 1 FROM bfs AS b JOIN call_edge_{tier} AS e ON e.src_fn_id = b.fn_id
  WHERE e.extern = 0 AND NOT EXISTS (SELECT 1 FROM recurring.bfs AS k WHERE k.anchor = b.anchor AND k.fn_id = e.dst_fn_id)
)
SELECT b.anchor, p.text AS target_path, n.text AS target_name, min(b.depth) AS length
FROM bfs AS b JOIN fn AS f ON f.id = b.target JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id
WHERE b.fn_id = b.target AND b.depth > 0 GROUP BY 1, 2, 3;
