"""Per (language, arm, anchor): fast/slow edge-key set agreement, written into results.db table agree.

usage: python3 2_agree.py
"""
import sqlite3, pathlib

db = sqlite3.connect(pathlib.Path(__file__).resolve().parent / "results.db")
db.executescript("""
drop table if exists agree;
create table agree as
with keys as (select distinct language, arm, anchor, tier, key from edge),
pairs as (
  select f.language, f.arm, f.anchor from run f join run s
    on s.language = f.language and s.arm = f.arm and s.anchor = f.anchor and f.tier = 'fast' and s.tier = 'slow')
select p.language, r.category, p.arm, p.anchor,
  (select count(*) from keys k where k.language = p.language and k.arm = p.arm and k.anchor = p.anchor and k.tier = 'fast'
     and exists (select 1 from keys o where o.language = k.language and o.arm = k.arm and o.anchor = k.anchor and o.tier = 'slow' and o.key = k.key)) as agree,
  (select count(*) from keys k where k.language = p.language and k.arm = p.arm and k.anchor = p.anchor and k.tier = 'fast'
     and not exists (select 1 from keys o where o.language = k.language and o.arm = k.arm and o.anchor = k.anchor and o.tier = 'slow' and o.key = k.key)) as fast_only,
  (select count(*) from keys k where k.language = p.language and k.arm = p.arm and k.anchor = p.anchor and k.tier = 'slow'
     and not exists (select 1 from keys o where o.language = k.language and o.arm = k.arm and o.anchor = k.anchor and o.tier = 'fast' and o.key = k.key)) as slow_only,
  (select exit_code from run x where x.language = p.language and x.arm = p.arm and x.anchor = p.anchor and x.tier = 'fast') as fast_exit,
  (select exit_code from run x where x.language = p.language and x.arm = p.arm and x.anchor = p.anchor and x.tier = 'slow') as slow_exit,
  (select round(seconds, 1) from run x where x.language = p.language and x.arm = p.arm and x.anchor = p.anchor and x.tier = 'fast') as fast_seconds,
  (select round(seconds, 1) from run x where x.language = p.language and x.arm = p.arm and x.anchor = p.anchor and x.tier = 'slow') as slow_seconds
from pairs p join run r on r.language = p.language and r.arm = p.arm and r.anchor = p.anchor and r.tier = 'fast';
""")
db.commit()
for row in db.execute("select language, arm, sum(agree), sum(fast_only), sum(slow_only), count(*) from agree group by 1, 2"):
    print(*row, sep="\t")
