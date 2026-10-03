-- @setup
CREATE PROPERTY GRAPH cst VERTEX TABLES (cst_vertex KEY (id))
  EDGE TABLES (chain_edge KEY (src, dst) SOURCE KEY (src) REFERENCES cst_vertex (id) DESTINATION KEY (dst) REFERENCES cst_vertex (id) LABEL chain);
CREATE PROPERTY GRAPH calls_fast_graph VERTEX TABLES (fn KEY (id))
  EDGE TABLES (calls_fast KEY (src_fn_id, dst_fn_id) SOURCE KEY (src_fn_id) REFERENCES fn (id) DESTINATION KEY (dst_fn_id) REFERENCES fn (id) LABEL calls);
-- @T3
SELECT s.anchor, r.fn_id FROM GRAPH_TABLE (calls_fast_graph
  MATCH ANY SHORTEST (a IS fn WHERE a.id IN (SELECT fn_id FROM seed))-[e IS calls]->{1,}(b IS fn)
  COLUMNS (a.id AS seed_fn_id, b.id AS fn_id)) AS r JOIN seed AS s ON s.fn_id = r.seed_fn_id;
