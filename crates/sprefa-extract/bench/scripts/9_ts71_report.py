"""python3 9_ts71_report.py PLANS_DIR -> markdown tables on stdout (medians of runs); task and method grids into PLANS_DIR."""
import json, sqlite3, statistics, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PLANS = Path(sys.argv[1])
db = sqlite3.connect(HERE.parent / "repos/ts71/ts71.db")
med = lambda xs: statistics.median(xs) if xs else None
ORDER = [r[0] for r in db.execute("select corpus from corpus")]
ORDER.sort(key=lambda c: -(db.execute("select max(files) from process where corpus=?", (c,)).fetchone()[0] or 0))


def table(cols, rows):
    out = ["| " + " | ".join(cols) + " |", "| " + " | ".join("---" for _ in cols) + " |"]
    for r in rows:
        out.append("| " + " | ".join("" if v is None else (f"{v:,.3f}" if abs(v) < 10 else f"{v:,.1f}") if isinstance(v, float) else f"{v:,}" if isinstance(v, int) else str(v) for v in r) + " |")
    return "\n".join(out)


def proc(c, tool, col="wall_ms", ok=True):
    q = f"select {col} from process where corpus=? and tool=?" + (" and exit_code=0" if ok else "")
    return med([r[0] for r in db.execute(q, (c, tool))])


def task(c, t, col="wall_ms"):
    return med([r[0] for r in db.execute(f"select {col} from task where corpus=? and task=?", (c, t)) if r[0] is not None])


def count(c, k, agg=None):
    vals = [r[0] for r in db.execute("select value from counts where corpus=? and key=?", (c, k))]
    return (med([float(v) for v in vals]) if agg else vals[0]) if vals else None


def kind(c, tool, rec):
    return med([r[0] for r in db.execute("select n from record_kind where corpus=? and tool=? and record=?", (c, tool, rec))])


def exits(c, tool):
    return ",".join(str(r[0]) for r in db.execute("select exit_code from process where corpus=? and tool=? order by run_ix", (c, tool)))


RESOLVE = ["init_spawn", "snapshot_open_projects", "program_source_file_names", "bulk_ast_fetch_and_walk", "bulk_call_symbols", "bulk_call_alias_targets"]


def ts_resolve_ms(c):
    xs = [task(c, t) for t in RESOLVE]
    return sum(xs) if all(x is not None for x in xs) else None


def ts_resolve_peak(c):
    xs = [task(c, t, "server_peak_rss_mb") for t in RESOLVE]
    return max(x for x in xs if x is not None) if any(x is not None for x in xs) else None


print("## Call resolution: ryi vs TS 7.1 API, median of runs\n")
print(table(["corpus", "files", "ryi_resolve_call_ms", "ryi_resolve_call_peak_rss_mb", "ts71_call_resolution_ms", "ts71_server_peak_rss_mb"],
            [(c, count(c, "files"), proc(c, "ryi_resolve_call"), proc(c, "ryi_resolve_call", "max_rss_mb"), ts_resolve_ms(c), ts_resolve_peak(c)) for c in ORDER]))

print("\n## Corpora\n")
print(table(["corpus", "commit", "projects", "files", "ast_nodes"],
            [(c, db.execute("select commit_sha from corpus where corpus=?", (c,)).fetchone()[0][:10], count(c, "projects"), count(c, "files"), count(c, "ast_nodes")) for c in ORDER]))
print("\n" + table(["corpus", "call_sites", "identifiers", "ts71_runs", "ts71_exit_codes"],
                   [(c, count(c, "call_sites"), count(c, "identifiers"), db.execute("select count(*) from process where corpus=? and tool='ts71_api'", (c,)).fetchone()[0], exits(c, "ts71_api")) for c in ORDER]))

for tool in ["ryi_fast", "ryi_resolve", "ryi_resolve_call", "ryi_slow"]:
    print(f"\n## `{tool}` process, median of runs with exit 0\n")
    print(table(["corpus", "wall_ms", "peak_rss_mb", "stdout_records", "exit_codes"],
                [(c, proc(c, tool), proc(c, tool, "max_rss_mb"), proc(c, tool, "stdout_records"), exits(c, tool)) for c in ORDER]))

print("\n## TS 7.1 API whole process (bulk phases + every method task + build), median of runs\n")
print(table(["corpus", "wall_ms", "node_peak_rss_mb", "tsgo_server_peak_rss_mb", "node_client_peak_rss_mb"],
            [(c, proc(c, "ts71_api"), proc(c, "ts71_api", "max_rss_mb"), count(c, "server_peak_rss_mb", True), count(c, "client_peak_rss_mb", True)) for c in ORDER]))

BULK = ["init_spawn", "snapshot_open_projects", "program_source_file_names", "bulk_ast_fetch", "bulk_ast_walk", "bulk_call_symbols",
        "bulk_call_alias_targets", "bulk_call_types", "bulk_identifier_symbols", "bulk_semantic_diagnostics"]
print("\n## TS 7.1 API bulk phases over every corpus file, median of runs (run order = row order)\n")
for c in ORDER:
    print(f"\n### {c}\n")
    print(table(["phase", "wall_ms", "server_peak_rss_mb", "client_peak_rss_mb", "server_end_rss_mb"],
                [(t, task(c, t), task(c, t, "server_peak_rss_mb"), task(c, t, "client_peak_rss_mb"), task(c, t, "server_end_rss_mb")) for t in BULK]))

print("\n## Call counts: ryi --resolve --arms call vs TS 7.1 getSymbolAtPosition on call names\n")
print(table(["corpus", "ts71_call_sites", "ts71_symbol_found", "ts71_decl_in_corpus", "ts71_semantic_diagnostics"],
            [(c, count(c, "call_sites"), count(c, "call_symbols"), count(c, "call_decl_in_corpus"), count(c, "semantic_diagnostics")) for c in ORDER]))
print("\n" + table(["corpus", "ryi_call_site", "ryi_resolved_edge", "ryi_unresolved"],
                   [(c, kind(c, "ryi_resolve_call", "call_site"), kind(c, "ryi_resolve_call", "resolved_edge"), kind(c, "ryi_resolve_call", "unresolved")) for c in ORDER]))

# grids: per task, per RPC method; medians across runs
task_rows = []
for c in ORDER:
    for t, in db.execute("select distinct task from task where corpus=?", (c,)):
        task_rows.append({"corpus": c, "task": t, "inputs": task(c, t, "inputs"), "errors": task(c, t, "errors"),
                          "wall_ms": round(task(c, t) or 0, 3), "server_peak_rss_mb": round(task(c, t, "server_peak_rss_mb") or 0, 1),
                          "client_peak_rss_mb": round(task(c, t, "client_peak_rss_mb") or 0, 1), "server_end_rss_mb": round(task(c, t, "server_end_rss_mb") or 0, 1),
                          "first_error": (db.execute("select first_error from task where corpus=? and task=? and first_error is not null limit 1", (c, t)).fetchone() or [""])[0][:200]})
method_rows = []
for c in ORDER:
    for method, in db.execute("select distinct method from rpc where corpus=? and method not like 'batched:%'", (c,)):
        per_run = {}
        for run_ix, calls, total, mc, t in db.execute("select run_ix, calls, total_ms, median_call_ms, task from rpc where corpus=? and method=?", (c, method)):
            r = per_run.setdefault(run_ix, {"calls": 0, "total": 0.0, "mc": [], "tasks": set()})
            r["calls"] += calls; r["total"] += total; r["mc"].append(mc); r["tasks"].add(t)
        tasks = sorted(set().union(*[r["tasks"] for r in per_run.values()]))
        peaks = [task(c, t, "server_peak_rss_mb") for t in tasks]
        method_rows.append({"corpus": c, "rpc_method": method, "calls_per_run": med([r["calls"] for r in per_run.values()]),
                            "total_ms": round(med([r["total"] for r in per_run.values()]), 3),
                            "median_call_ms": round(med([med(r["mc"]) for r in per_run.values()]), 4),
                            "server_peak_rss_mb": round(max([p for p in peaks if p is not None], default=0), 1),
                            "tasks": " ".join(tasks)})
methods = [m for m, in db.execute("select method from api_method")]
seen = {r["rpc_method"] for r in method_rows}
for m in methods:
    if m not in seen:
        method_rows.append({"corpus": "(none)", "rpc_method": m, "calls_per_run": 0, "total_ms": None, "median_call_ms": None, "server_peak_rss_mb": None, "tasks": ""})
for rows, name, title in [(task_rows, "tasks", "TS 7.1 API tasks: wall time and peak RSS, median of runs"),
                          (method_rows, "methods", "TS 7.1 API RPC methods: calls, time, peak server RSS of issuing tasks, median of runs")]:
    tmp = HERE.parent / f"repos/ts71/{name}.json"
    tmp.write_text(json.dumps(rows))
    subprocess.run(["python3", str(HERE.parents[3] / "scripts/bench_grid.py"), "--json", str(tmp), "--out", str(PLANS / f"ts71-api-bench.{name}.html"), "--title", title], check=True, stdout=subprocess.DEVNULL)

print("\n## Method coverage\n")
print(table(["metric", "value"], [("APIMethodInfo methods", len(methods)), ("methods called on at least one corpus", len(seen & set(methods))),
                                  ("methods never called", ", ".join(sorted(set(methods) - seen)))]))

print("\n## Task errors (any run)\n")
print(table(["corpus", "task", "errors", "inputs", "first_error"],
            [(c, t, e, i, (fe or "").splitlines()[0][:110]) for c, t, e, i, fe in
             db.execute("select corpus, task, max(errors), max(inputs), first_error from task where errors>0 group by corpus, task order by task, corpus")]))

SEL = ["getSymbolAtPosition", "getTypeAtPosition", "getResolvedSignature", "typeToString", "isTypeAssignableTo", "getReferencesToSymbolInFile",
       "getReferencedSymbolsForNode", "getSignatureUsage", "getSymbolsInScope", "getCompletionsAtPosition", "getImportEditsForSymbols",
       "getSemanticDiagnostics", "getJavaScriptEmit", "runWithTemporaryFileUpdate+semanticDiagnostics", "api.batch(getSymbolAtPosition)",
       "createProgram", "emit", "build"]
print("\n## Selected method tasks, median of runs\n")
for c in ORDER:
    print(f"\n### {c}\n")
    print(table(["task", "inputs", "wall_ms", "ms_per_input", "server_peak_rss_mb"],
                [(t, task(c, t, "inputs"), task(c, t), (task(c, t) or 0) / (task(c, t, "inputs") or 1), task(c, t, "server_peak_rss_mb")) for t in SEL]))
