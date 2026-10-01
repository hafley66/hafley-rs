-- Input: ryii --resolve --rust-checker --kinds call --sqlite over crates/boop*, then 0_test_scope.sql.
-- `unresolved.detail` holds rust-analyzer's crate-qualified callee path.

drop table if exists effect_rule;
create table effect_rule (pattern text not null, effect text not null);
insert into effect_rule values
  ('std::process::%', 'process'), ('tokio::process::%', 'process'), ('wait_timeout::%', 'process'),
  ('sysinfo::%', 'process'), ('libc::%', 'process'), ('nix::%', 'process'), ('signal_hook::%', 'signal'),
  ('std::fs::%', 'fs'), ('std::os::unix::fs::%', 'fs'), ('tempfile::%', 'fs'), ('ignore::%', 'fs'), ('dirs::%', 'fs'),
  ('std::path::Path::exists', 'fs'), ('std::path::Path::is_file', 'fs'), ('std::path::Path::is_dir', 'fs'),
  ('std::path::Path::metadata', 'fs'), ('std::path::Path::read_dir', 'fs'), ('std::path::Path::canonicalize', 'fs'),
  ('std::path::Path::try_exists', 'fs'), ('std::path::Path::symlink_metadata', 'fs'),
  ('rusqlite::%', 'sqlite'),
  ('tmux_interface::%', 'tmux'),
  ('ureq::%', 'http'), ('http::%', 'http'),
  ('tungstenite::%', 'ws'), ('tokio_tungstenite::%', 'ws'),
  ('std::net::%', 'net'), ('tokio::net::%', 'net'),
  ('agent_client_protocol::%', 'jsonrpc'),
  ('std::io::stdin%', 'stdio'), ('std::io::stdout%', 'stdio'), ('std::io::stderr%', 'stdio'),
  ('std::io::Stdin::%', 'stdio'), ('std::io::Stdout::%', 'stdio'), ('std::io::Stderr::%', 'stdio'),
  ('std::io::IsTerminal::%', 'stdio'),
  ('std::env::%', 'env'),
  ('std::time::SystemTime::now', 'clock'), ('std::time::Instant::now', 'clock'), ('time::%::now_%', 'clock'),
  ('std::thread::sleep', 'clock'),
  ('std::thread::%', 'thread'), ('tokio::%spawn%', 'task'), ('tokio::runtime::%', 'task'), ('tokio::task::%', 'task'),
  ('crossbeam%', 'channel'), ('std::sync::mpsc::%', 'channel'), ('tokio::sync::%', 'channel'),
  ('std::sync::%::lock', 'lock'), ('std::sync::%::read', 'lock'), ('std::sync::%::write', 'lock'),
  ('bumpalo::%', 'arena'), ('typed_arena::%', 'arena'), ('slab::%', 'arena'), ('la_arena::%', 'arena'),
  ('tracing::%', 'log'), ('tracing_subscriber::%', 'log'), ('hafley_observe::%', 'log');

create index if not exists node_owner on node(_input_path, family, kind, span__start);

drop table if exists effect_site;
create table effect_site as
with prod as (
  select u.path, u.span__start as start, u.span__end as end, u.detail as callee
  from unresolved u
  where u.reason = 'external'
    and exists (select 1 from effect_rule r where u.detail like r.pattern)
    and u.path like 'crates/%/src/%'
    and u.path not like '%/tests/%'
    and not exists (
      select 1 from test_scope c
      where c.path = u.path
        and u.span__start between c.span__start and c.span__end)
)
select
  substr(p.path, 8, instr(substr(p.path, 8), '/') - 1) as crate,
  p.path,
  (select length(t) - length(replace(t, x'0a', '')) + 1
     from (select substr(cast(readfile(p.path) as text), 1, p.start) as t)) as line,
  (select n.name from node n
     where n._input_path = p.path and n.family = 'call' and n.kind in ('function', 'method')
       and p.start between n.span__start and n.span__end
     order by n.span__end - n.span__start limit 1) as owner,
  (select r.effect from effect_rule r where p.callee like r.pattern limit 1) as effect,
  p.callee,
  -- Call arguments as written, for program names and SQL text.
  replace(replace(substr(cast(readfile(p.path) as text), p.end + 1, 60), x'0a', ' '), '    ', '') as written_after
from prod p;
delete from effect_site where effect is null;

drop table if exists effect_crate;
create table effect_crate as
select crate, effect, count(*) as sites, count(distinct owner) as owners, count(distinct path) as files
from effect_site group by 1, 2;

-- Effects a function reaches through corpus calls; (path, name) is the function key.
create index if not exists resolved_edge_callee on resolved_edge(callee_path, callee_name);
drop table if exists fn_effect;
create table fn_effect as
with recursive reach(path, name, effect) as (
  select distinct path, owner, effect from effect_site where owner is not null
  union
  select e.caller_path, e.caller_name, r.effect
  from reach r join resolved_edge e on e.callee_path = r.path and e.callee_name = r.name
  where e.caller_name is not null and e.caller_path not like '%/tests/%'
)
select * from reach;
