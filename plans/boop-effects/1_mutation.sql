-- Functions and closures with no mutation, after 0_effects.sql.
-- Gap: a closure mutating a capture (`v.push(x)` on an outer `let mut v`) carries no marker in its own span.

create index if not exists node_kind on node(family, kind, _input_path, span__start);

drop table if exists unit;
create table unit as
select n._input_path as path, n.span__start as start, n.span__end as end, n.kind, n.name
from node n
where n.family = 'call' and n.kind in ('function', 'method', 'lambda')
  and n._input_path like 'crates/%/src/%'
  and not exists (
    select 1 from test_scope c
    where c.path = n._input_path
      and n.span__start between c.span__start and c.span__end);

drop table if exists mutation_site;
create table mutation_site as
select m._input_path as path,
  length(cast(substr(cast(f.text as blob), 1, m.span__start) as text)) as start, m.kind as marker
from node m join file_text f on f.path = m._input_path
where m.family = 'cst'
  and m.kind in ('mutable_specifier', 'assignment_expression', 'compound_assignment_expr', 'unsafe_block')
union all
select u.path, u.span__start, 'interior:' || u.detail
from unresolved u
where u.reason = 'external'
  and (u.detail like '%::RefCell::borrow_mut' or u.detail like '%::RefCell::replace%'
    or u.detail like '%::Cell::set' or u.detail like '%::Cell::replace' or u.detail like '%::Cell::take'
    or u.detail like '%::Mutex::lock' or u.detail like '%::RwLock::write'
    or u.detail like '%::atomic::%::store' or u.detail like '%::atomic::%::fetch_%'
    or u.detail like '%::atomic::%::swap' or u.detail like '%::atomic::%::compare_exchange%'
    or u.detail like '%::OnceCell::set' or u.detail like '%::OnceLock::set');
create index mutation_site_at on mutation_site(path, start);

-- Direct: a marker or an effect site inside the unit's own span.
drop table if exists unit_direct;
create table unit_direct as
select x.path, x.start, x.end, x.kind, x.name,
  (select count(*) from mutation_site m where m.path = x.path and m.start between x.start and x.end) as markers,
  (select count(*) from effect_site e
     where e.path = x.path and e.effect not in ('log', 'clock', 'env')
       and e.line between
         (select length(t) - length(replace(t, x'0a', '')) + 1 from (select substr((select text from file_text where path = x.path), 1, x.start) as t))
         and
         (select length(t) - length(replace(t, x'0a', '')) + 1 from (select substr((select text from file_text where path = x.path), 1, x.end) as t))
  ) as effects
from unit x;

-- Transitive: a named unit is impure when any corpus callee is.
drop table if exists impure;
create table impure as
with recursive up(path, name) as (
  select distinct path, name from unit_direct where (markers > 0 or effects > 0) and name is not null
  union
  select e.caller_path, e.caller_name
  from up join resolved_edge e on e.callee_path = up.path and e.callee_name = up.name
  where e.caller_name is not null
)
select * from up;

drop table if exists pure_unit;
create table pure_unit as
select d.* from unit_direct d
where d.markers = 0 and d.effects = 0
  and not exists (select 1 from impure i where i.path = d.path and i.name = d.name);
