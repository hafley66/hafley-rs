create index if not exists edge_from on edge(_input_path, family, kind, from__start, from__end);
create index if not exists edge_to on edge(_input_path, family, kind, to__start, to__end);
create index if not exists node_span on node(_input_path, family, span__start, span__end);

drop table if exists file_text;
create table file_text (path text primary key, text text);
insert into file_text select path, cast(readfile(path) as text) from (select distinct _input_path as path from node where family = 'cst');

-- An item is test code when an attribute in the run before it reads #[cfg(...test...)].
drop table if exists test_scope;
create table test_scope as
with attr as materialized (
  select n._input_path as path, n.span__start as s, n.span__end as e
  from node n join file_text f on f.path = n._input_path
  where n.family = 'cst' and n.kind = 'attribute_item'
    and cast(substr(cast(f.text as blob), n.span__start + 1, n.span__end - n.span__start) as text) like '#[cfg(%test%'
), parent as materialized (
  select a.path, a.e, p.from__start as ps, p.from__end as pe
  from attr a join edge p
    on p._input_path = a.path and p.family = 'cst' and p.kind = 'child'
   and p.to__start = a.s and p.to__end = a.e
)
select p.path, min(c.to__start) as span__start, c.to__end as span__end
from parent p join edge c
  on c._input_path = p.path and c.family = 'cst' and c.kind = 'child'
 and c.from__start = p.ps and c.from__end = p.pe and c.to__start >= p.e
where not exists (
  select 1 from node k
  where k._input_path = c._input_path and k.family = 'cst'
    and k.span__start = c.to__start and k.span__end = c.to__end
    and k.kind in ('attribute_item', 'line_comment', 'block_comment'))
group by p.path, p.e;
