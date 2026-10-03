"""Run every names.tsv anchor through `ryii graph` fast and --slow; land runs and edge keys in results.db.

usage: python3 1_run.py [WORKERS]   (resumable: a (lang, arm, anchor, tier) already in run is skipped)
"""
import json, os, sqlite3, subprocess, sys, time, pathlib, threading
from concurrent.futures import ThreadPoolExecutor

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent.parent
WT = BENCH.parent.parent.parent
RYII = BENCH.parent / "target" / "release" / "ryii"
TS5 = "/Users/chrishafley/projects/hafley-alloy/node_modules/typescript/lib/typescript.js"
CORPUS = {
    "rust": (WT, ["--root", "crates/hafley_scm", "crates/hafley_scm"]),
    "ts": (BENCH / "repos" / "hafley-rxjs-main", ["--root", ".", "packages"]),
}
ARMS = {"call": ["callers", "from", "call-path"], "type": ["uses", "type-path"]}
KEY = {
    "graph_edge": ("from_path", "from_name", "from_line", "to_path", "to_name"),
    "graph_node": ("path", "name"),
    "graph_path": ("from_path", "from_name", "to_path", "to_name"),
    "graph_decline": ("from_path", "from_name", "type_name", "crate_name"),
}
db = sqlite3.connect(HERE / "results.db", check_same_thread=False)
lock = threading.Lock()
db.executescript("""
create table if not exists run(language text, category text, arm text, anchor text, tier text,
  exit_code integer, seconds real, rows integer, stderr_tail text);
create table if not exists edge(language text, arm text, anchor text, tier text, record text, key text, depth integer);
""")


def one(job):
    lang, category, arm, anchor, tier = job
    cwd, tail = CORPUS[lang]
    argv = [str(RYII), "graph", f"--{arm}", anchor, "--timeout", "300", *tail]
    if tier == "slow":
        argv.insert(2, "--slow")
    env = dict(os.environ, RUST_LOG="warn", SPREFA_TS_CHECKER_TYPESCRIPT=TS5)
    started = time.time()
    try:
        done = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=900)
        code, out, err = done.returncode, done.stdout, done.stderr
    except subprocess.TimeoutExpired as expired:
        code, out, err = -9, expired.stdout or "", "wall timeout 900s"
        out = out.decode() if isinstance(out, bytes) else out
    seconds = time.time() - started
    rows = [json.loads(line) for line in out.splitlines() if line.startswith("{")]
    with lock:
        db.execute("insert into run values (?,?,?,?,?,?,?,?,?)",
                   (lang, category, arm, anchor, tier, code, seconds, len(rows), err[-600:]))
        db.executemany("insert into edge values (?,?,?,?,?,?,?)", [
            (lang, arm, anchor, tier, row["record"],
             json.dumps([row.get(field) for field in KEY.get(row["record"], sorted(row))]), row.get("depth"))
            for row in rows])
        db.commit()
    print(f"{lang} {arm} {anchor} {tier} exit={code} {seconds:.1f}s rows={len(rows)}", flush=True)


def jobs():
    seen = set(db.execute("select language, arm, anchor, tier from run"))
    lines = (HERE / "names.tsv").read_text().splitlines()[1:]
    for line in lines:
        lang, category, plane, anchor = line.split("\t")
        for arm in ARMS[plane]:
            if "#" in anchor and arm == "uses":
                continue
            for tier in ("fast", "slow"):
                if (lang, arm, anchor, tier) not in seen:
                    yield (lang, category, arm, anchor, tier)


if __name__ == "__main__":
    workers = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    with ThreadPoolExecutor(workers) as pool:
        list(pool.map(one, list(jobs())))
