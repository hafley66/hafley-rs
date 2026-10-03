-- @setup
CREATE PROPERTY GRAPH cst VERTEX TABLES (cst_vertex)
  EDGE TABLES (chain_edge SOURCE KEY (src) REFERENCES cst_vertex (id) DESTINATION KEY (dst) REFERENCES cst_vertex (id) LABEL chain);
CREATE PROPERTY GRAPH calls_fast_graph VERTEX TABLES (fn)
  EDGE TABLES (calls_fast SOURCE KEY (src_fn_id) REFERENCES fn (id) DESTINATION KEY (dst_fn_id) REFERENCES fn (id) LABEL calls);
CREATE PROPERTY GRAPH calls_slow_graph VERTEX TABLES (fn)
  EDGE TABLES (calls_slow SOURCE KEY (src_fn_id) REFERENCES fn (id) DESTINATION KEY (dst_fn_id) REFERENCES fn (id) LABEL calls);
-- @T1
WITH link AS MATERIALIZED (
  FROM GRAPH_TABLE (cst
    MATCH ANY SHORTEST (o:cst_vertex WHERE o.kind = (SELECT id FROM scmpp_dict_kind WHERE text = 'call_expression'))
      -[e:chain]->{1,}(l:cst_vertex WHERE l.kind = (SELECT id FROM scmpp_dict_kind WHERE text = 'call_expression'))
    WHERE o.file = l.file
    COLUMNS (o.file AS file, o.pre AS origin, l.pre AS pre, o.start AS outer_start, o."end" AS outer_end,
      l.start AS link_start, l."end" AS link_end))
)
SELECT p.text AS path, x.outer_start, x.outer_end, x.link_start, x.link_end FROM link AS x
JOIN scmpp_dict_path AS p ON p.id = x.file
WHERE NOT EXISTS (SELECT 1 FROM link AS y WHERE y.file = x.file AND y.pre = x.origin);
-- @T3
WITH r AS (FROM GRAPH_TABLE (calls_{tier}_graph
  MATCH ANY SHORTEST (a:fn WHERE a.id IN (SELECT fn_id FROM seed))-[e:calls]->{1,}(b:fn)
  COLUMNS (a.id AS seed_fn_id, b.id AS fn_id)))
SELECT s.anchor, p.text AS path, n.text AS name FROM r
JOIN seed AS s ON s.fn_id = r.seed_fn_id JOIN fn AS f ON f.id = r.fn_id
JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id;
-- @T4
WITH r AS (FROM GRAPH_TABLE (calls_{tier}_graph
  MATCH q = ANY SHORTEST (a:fn WHERE a.id IN (SELECT seed_fn_id FROM t4_pair WHERE tier = '{tier}'))
    -[e:calls]->{1,}(b:fn WHERE b.id IN (SELECT target_fn_id FROM t4_pair WHERE tier = '{tier}'))
  COLUMNS (a.id AS seed_fn_id, b.id AS target_fn_id, path_length(q) AS length)))
SELECT t.anchor, p.text AS target_path, n.text AS target_name, r.length FROM r
JOIN t4_pair AS t ON t.tier = '{tier}' AND t.seed_fn_id = r.seed_fn_id AND t.target_fn_id = r.target_fn_id
JOIN fn AS f ON f.id = r.target_fn_id JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id;
