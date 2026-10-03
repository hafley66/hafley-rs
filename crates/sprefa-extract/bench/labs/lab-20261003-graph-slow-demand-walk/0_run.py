"""Rust call anchors of lab-20261002-ryi-graph-fast-vs-slow/names.tsv through `ryii graph --slow --from`:
tier `eager` = ryii built at f4e19b7d (whole-project slow store), tier `walk` = this branch (demand walk).
That lab's corpus and argv narrowed to its Rust files (`--pattern '**/*.rs'`: the crate holds one .mjs, and a
bare NAME over a mixed corpus stays eager); edge keys as its 1_run.py; agreement as its 2_agree.py.

usage: python3 0_run.py [WORKERS]   (resumable)  ->  results.db (run, edge, agree)
"""
import json, os, sqlite3, subprocess, sys, time, pathlib, threading
from concurrent.futures import ThreadPoolExecutor

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent.parent
WT = BENCH.parent.parent.parent
NAMES = HERE.parent / "lab-20261002-ryi-graph-fast-vs-slow" / "names.tsv"
BINARY = {"eager": HERE / "ryii-eager-f4e19b7d", "walk": BENCH.parent / "target" / "release" / "ryii"}
ARMS = ("from",)
KEY = {"graph_node": ("path", "name"), "graph_path": ("from_path", "from_name", "to_path", "to_name")}
db = sqlite3.connect(HERE / "results.db", check_same_thread=False)
lock = threading.Lock()
db.executescript("""
create table if not exists run(category text, arm text, anchor text, tier text,
  exit_code integer, seconds real, max_rss_bytes integer, rows integer, stderr_tail text);
create table if not exists edge(arm text, anchor text, tier text, record text, key text, depth integer);
""")


def one(job):
    category, arm, anchor, tier = job
    argv = ["/usr/bin/time", "-l", str(BINARY[tier]), "graph", "--slow", f"--{arm}", anchor,
            "--timeout", "300", "--root", "crates/hafley_scm", "--pattern", "**/*.rs", "crates/hafley_scm"]
    env = dict(os.environ, RUST_LOG="warn")
    started = time.time()
    done = subprocess.run(argv, cwd=WT, env=env, capture_output=True, text=True, timeout=900)
    seconds = time.time() - started
    rss = next((int(line.split()[0]) for line in done.stderr.splitlines() if "maximum resident set size" in line), None)
    rows = [json.loads(line) for line in done.stdout.splitlines() if line.startswith("{")]
    with lock:
        db.execute("insert into run values (?,?,?,?,?,?,?,?,?)",
                   (category, arm, anchor, tier, done.returncode, seconds, rss, len(rows), done.stderr[-900:]))
        db.executemany("insert into edge values (?,?,?,?,?,?)", [
            (arm, anchor, tier, row["record"], json.dumps([row.get(field) for field in KEY[row["record"]]]), row.get("depth"))
            for row in rows])
        db.commit()
    print(f"{arm} {anchor} {tier} exit={done.returncode} {seconds:.1f}s rows={len(rows)}", flush=True)


def jobs():
    seen = set(db.execute("select arm, anchor, tier from run"))
    for line in NAMES.read_text().splitlines()[1:]:
        lang, category, plane, anchor = line.split("\t")
        if lang != "rust" or plane != "call":
            continue
        for arm in ARMS:
            for tier in BINARY:
                if (arm, anchor, tier) not in seen:
                    yield (category, arm, anchor, tier)


AGREE = """
drop table if exists agree;
create table agree as
with keys as (select distinct arm, anchor, tier, key from edge),
pairs as (select e.category, e.arm, e.anchor from run e join run w
  on w.arm = e.arm and w.anchor = e.anchor and e.tier = 'eager' and w.tier = 'walk')
select p.category, p.arm, p.anchor,
  (select count(*) from keys k where k.arm = p.arm and k.anchor = p.anchor and k.tier = 'eager'
     and exists (select 1 from keys o where o.arm = k.arm and o.anchor = k.anchor and o.tier = 'walk' and o.key = k.key)) as agree,
  (select count(*) from keys k where k.arm = p.arm and k.anchor = p.anchor and k.tier = 'eager'
     and not exists (select 1 from keys o where o.arm = k.arm and o.anchor = k.anchor and o.tier = 'walk' and o.key = k.key)) as eager_only,
  (select count(*) from keys k where k.arm = p.arm and k.anchor = p.anchor and k.tier = 'walk'
     and not exists (select 1 from keys o where o.arm = k.arm and o.anchor = k.anchor and o.tier = 'eager' and o.key = k.key)) as walk_only,
  (select exit_code from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'eager') as eager_exit,
  (select exit_code from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'walk') as walk_exit,
  (select round(seconds, 1) from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'eager') as eager_seconds,
  (select round(seconds, 1) from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'walk') as walk_seconds,
  (select max_rss_bytes / 1048576 from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'eager') as eager_rss_mb,
  (select max_rss_bytes / 1048576 from run x where x.arm = p.arm and x.anchor = p.anchor and x.tier = 'walk') as walk_rss_mb
from pairs p;
"""

if __name__ == "__main__":
    workers = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    with ThreadPoolExecutor(workers) as pool:
        list(pool.map(one, list(jobs())))
    db.executescript(AGREE)
    db.commit()
    for row in db.execute("select * from agree order by category, anchor"):
        print(*row, sep="\t")
