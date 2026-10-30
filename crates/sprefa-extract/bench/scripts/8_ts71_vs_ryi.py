"""python3 8_ts71_vs_ryi.py [--runs 3] [--sample 200] [CORPUS...] -> bench/repos/ts71/ts71.db
Per corpus and run: the 7_ts71_api_bench.mjs process, then ryii fast / --resolve / --resolve --arms call / slow on the same files."""
import argparse, json, os, re, sqlite3, subprocess, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
OUT = BENCH / "repos/ts71"
RYI = os.environ.get("RYI_BIN", os.path.expanduser("~/.cargo/bin/ryii"))
RYI_ARMS = {"ryi_fast": ["fast"], "ryi_resolve": ["--resolve"], "ryi_resolve_call": ["--resolve", "--arms", "call"], "ryi_slow": ["slow"]}

ap = argparse.ArgumentParser()
ap.add_argument("--runs", type=int, default=3)
ap.add_argument("--sample", default="200")
ap.add_argument("corpora", nargs="*")
args = ap.parse_args()

db = sqlite3.connect(OUT / "ts71.db")
db.executescript("""
create table if not exists process(corpus, tool, run_ix, wall_ms, max_rss_mb, files, stdout_records, exit_code, stderr_tail);
create table if not exists record_kind(corpus, tool, run_ix, record, n);
create table if not exists task(corpus, run_ix, task, inputs, ok, errors, wall_ms, first_error, server_peak_rss_mb, client_peak_rss_mb, server_end_rss_mb);
create table if not exists rpc(corpus, run_ix, task, method, calls, total_ms, median_call_ms);
create table if not exists counts(corpus, run_ix, key, value);
create table if not exists api_method(method);
create table if not exists corpus(corpus, dir, commit_sha, tsconfigs);
""")


def timed(cmd, cwd, stdout_path, stderr_path):
    t0 = time.perf_counter()
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as errf:
        p = subprocess.run(["/usr/bin/time", "-l", *cmd], cwd=cwd, stdout=out, stderr=errf,
                           env={**os.environ, "RUST_LOG": "error"})
    real = (time.perf_counter() - t0) * 1000
    err = Path(stderr_path).read_text(errors="replace")
    rss = int(re.search(r"(\d+)\s+maximum resident set size", err).group(1)) / 2**20
    tail = "\n".join(l for l in err.splitlines() if not l.startswith(" ") and not l.startswith("task ") and "real" not in l)[-400:]
    return real, rss, p.returncode, tail


rows = [l.split("\t") for l in (HERE / "7_ts71_corpora.tsv").read_text().splitlines()[1:]]
for name, rel, cfgs in rows:
    if args.corpora and name not in args.corpora:
        continue
    root = (BENCH / rel).resolve()
    sha = subprocess.run(["git", "-C", root, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    for t in ["process", "record_kind", "task", "rpc", "counts", "corpus"]:
        db.execute(f"delete from {t} where corpus=?", (name,))
    db.execute("insert into corpus values (?,?,?,?)", (name, rel, sha, cfgs))
    files_txt = OUT / f"{name}.files"
    scratch = OUT / "scratch" / name
    for run_ix in range(1, args.runs + 1):
        js = OUT / f"{name}.run{run_ix}.json"
        cmd = ["node", "--max-old-space-size=10240", str(HERE / "7_ts71_api_bench.mjs"), "--root", str(root), "--out", str(js), "--sample", args.sample,
               "--scratch", str(scratch), "--files-out", str(files_txt), *cfgs.split()]
        real, rss, code, tail = timed(cmd, root, OUT / "api.stdout", OUT / f"{name}.ts71_api.run{run_ix}.stderr")
        files = [f[len(str(root)) + 1:] for f in files_txt.read_text().split()] if files_txt.exists() else []
        db.execute("insert into process values (?,?,?,?,?,?,?,?,?)", (name, "ts71_api", run_ix, real, rss, len(files), None, code, tail))
        if code == 0:
            d = json.loads(js.read_text())
            for t in d["tasks"]:
                db.execute("insert into task values (?,?,?,?,?,?,?,?,?,?,?)", (name, run_ix, t["task"], t["inputs"], t["ok"], t["errors"], t["wall_ms"], t.get("first_error"),
                                                                     t.get("server_peak_rss_mb"), t.get("client_peak_rss_mb"), t.get("server_end_rss_mb")))
            for r in d["rpc"]:
                db.execute("insert into rpc values (?,?,?,?,?,?,?)", (name, run_ix, r["task"], r["method"], r["calls"], r["total_ms"], r["median_call_ms"]))
            for k, v in d["counts"].items():
                db.execute("insert into counts values (?,?,?,?)", (name, run_ix, k, v))
            db.execute("delete from api_method")
            db.executemany("insert into api_method values (?)", [(m,) for m in d["api_methods"]])
        for tool, argv in RYI_ARMS.items():
            out = OUT / f"{tool}.stdout"
            real, rss, code, tail = timed([RYI, *argv, "--root", ".", *files], root, out, OUT / f"{name}.{tool}.run{run_ix}.stderr")
            kinds = {}
            for line in out.open("rb"):
                try:
                    k = json.loads(line).get("record")
                except ValueError:
                    k = "<non-json>"
                kinds[k] = kinds.get(k, 0) + 1
            db.execute("insert into process values (?,?,?,?,?,?,?,?,?)", (name, tool, run_ix, real, rss, len(files), sum(kinds.values()), code, tail))
            db.executemany("insert into record_kind values (?,?,?,?,?)", [(name, tool, run_ix, k, n) for k, n in kinds.items()])
        db.commit()
        print(name, run_ix, time.strftime("%H:%M:%S"), flush=True)
