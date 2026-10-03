"""db/raw.tsv + per-run outputs -> results.tsv (one row per run/case/corpus/engine/variant, medians of repeats),
summaries (c1, x1, w1) appended as their own result rows, grid.html (Q1 per-case table and every other row).

usage: python3 9_compare.py
"""
import csv, json, pathlib, statistics, subprocess, collections

HERE = pathlib.Path(__file__).resolve().parent
DB = HERE / "db"
SQLITE = "/opt/homebrew/opt/sqlite/bin/sqlite3"
DUCKDB = "/opt/homebrew/bin/duckdb"
COLUMNS = ["run", "case", "corpus", "engine", "variant", "repeats", "load_average_1m", "wall_s", "peak_rss_mb", "status",
           "rows", "reference_rows", "rows_equal", "order_equal", "metric", "value", "note"]


def tsv(path):
    if not path.exists():
        return []
    with path.open() as handle:
        return list(csv.DictReader(handle, delimiter="\t", quoting=csv.QUOTE_NONE))


def median(values):
    values = [float(v) for v in values if v not in ("", None)]
    return f"{statistics.median(values):.3f}" if values else ""


def rows_file(path):
    data = path.read_bytes()
    rows = data.split(b"\x1e") if b"\x1e" in data else data.split(b"\n")
    return [row for row in rows if row.strip()]


def row(**fields):
    return {column: str(fields.get(column, "")) for column in COLUMNS}


RECURSION_OUT = pathlib.Path("/Users/chrishafley/projects/hafley-rs/.boop-worktrees/lab/recursion/crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa/db/out")


def m1_reference(case, corpus):
    """ancestor: SQLite rows of L1; chain: the recursion lab's SQLite (R1) T1 output, header excluded."""
    if case == "ancestor":
        path = DB / "out" / "L1" / corpus / "ancestor.sqlite-query.rows"
        return str(len(rows_file(path))) if path.exists() else ""
    path = RECURSION_OUT / f"R1_T1_{corpus}_cst.tsv"
    return str(sum(1 for _ in path.open()) - 1) if path.exists() else ""


def measured():
    groups = collections.OrderedDict()
    for line in tsv(DB / "raw.tsv"):
        variant = "process" if line["run"] == "W1" else line["variant"]  # W1 repeats are separate processes r1..r3
        groups.setdefault((line["run"], line["case"], line["corpus"], line["engine"], variant), []).append(line)
    q1 = {(line["case"], line["corpus"]): line for corpus in ("small", "crates") for line in tsv(DB / f"q1-{corpus}.tsv")}
    out = []
    for (run, case, corpus, engine, variant), lines in groups.items():
        ok = [line for line in lines if line["status"] == "ok"]
        fields = dict(run=run, case=case, corpus=corpus, engine=engine, variant=variant, repeats=len(lines),
                      load_average_1m=lines[0]["load_average_1m"], wall_s=median(l["wall_s"] for l in ok),
                      peak_rss_mb=median(l["peak_rss_mb"] for l in ok), status="ok" if len(ok) == len(lines) else "error",
                      note="; ".join(sorted({l["note"] for l in lines if l["note"]}))[:240])
        if run == "Q1" and variant == "query" and (case, corpus) in q1:
            summary = q1[(case, corpus)]
            fields.update(rows=summary[f"{engine}_rows"], reference_rows=summary["sqlite_rows"],
                          rows_equal="reference" if engine == "sqlite" else summary["rows_equal"],
                          order_equal="reference" if engine == "sqlite" else summary["order_equal"])
        if run in ("Q2", "L1") and "query" in variant:
            output = DB / "out" / run / corpus / f"{case}.{engine}-{variant}.rows"
            reference = DB / "out" / run / corpus / f"{case}.sqlite-query.rows"
            if output.exists() and reference.exists():
                got, want = rows_file(output), rows_file(reference)
                fields.update(rows=len(got), reference_rows=len(want),
                              rows_equal="reference" if engine == "sqlite" else ("yes" if sorted(got) == sorted(want) else "no"),
                              order_equal="reference" if engine == "sqlite" else ("yes" if got == want else "no"))
        if run == "M1":
            text = (HERE / lines[-1]["output"]).read_text().split()
            reference = m1_reference(case, corpus)
            if len(text) >= 2:
                fields.update(rows=text[0], reference_rows=reference, rows_equal="yes" if text[0] == reference else "no",
                              metric="temporary_storage_bytes", value=text[1])
        if run == "W1":
            outputs = [(HERE / l["output"]).read_text().split("\t") for l in ok if (HERE / l["output"]).exists()]
            outputs = [o for o in outputs if len(o) == 8]
            if outputs and engine != "none":
                fields.update(rows=sum(int(x) for x in outputs[0][1:4]),
                              metric="write_s|index_or_checkpoint_s|rows_per_s",
                              value="|".join(median(o[i] for o in outputs) for i in (5, 6, 7)))
        out.append(row(**fields))
    return out


def w1_rows(results):
    """W1 split into one metric per row (1NF), plus the written file's row counts against the input store."""
    out = []
    for result in [r for r in results if r["run"] == "W1"]:
        if result["metric"]:
            for metric, value in zip(result["metric"].split("|"), result["value"].split("|")):
                out.append(row(**{**result, "metric": metric, "value": value}))
            result["metric"] = result["value"] = ""
        corpus, engine = result["corpus"], result["engine"]
        written = DB / f"w1-{corpus}.{engine}"
        if written.exists():
            query = "SELECT (SELECT count(*) FROM scmpp_node) + (SELECT count(*) FROM scmpp_capture)"
            cli = [SQLITE, str(written), query] if engine == "sqlite" else [DUCKDB, "-readonly", "-noheader", "-list", str(written), "-c", query]
            got = subprocess.run(cli, capture_output=True, text=True).stdout.strip()
            want = subprocess.run([SQLITE, str(DB / f"ancestor-{corpus}.db"), query], capture_output=True, text=True).stdout.strip()
            out.append(row(**{**{k: result[k] for k in ("run", "case", "corpus", "engine", "variant")}, "metric": "file_bytes",
                              "value": written.stat().st_size + sum(p.stat().st_size for p in DB.glob(f"w1-{corpus}.{engine}.wal")),
                              "rows": got, "reference_rows": want, "rows_equal": "yes" if got == want else "no"}))
    return out


def c1_rows():
    groups = collections.defaultdict(list)
    for line in tsv(DB / "c1.tsv"):
        groups[(line["engine"], line["corpus"], line["iteration"])].append(line)
    return [row(run="C1", case="ancestor", corpus=corpus, engine=engine, variant=f"iteration{iteration}", repeats=len(lines),
                load_average_1m=lines[0]["load_average_1m"], wall_s=median(l["wall_s"] for l in lines), status="ok",
                note="in-process statement time; iteration1 = first query after process start")
            for (engine, corpus, iteration), lines in sorted(groups.items())]


def x1_rows():
    out = []
    runs = {line["engine"]: line for line in tsv(DB / "x1-run.tsv")}
    writer = collections.defaultdict(list)
    for line in tsv(DB / "x1-writer.tsv"):
        writer[line["engine"]].append(line)
    readers = collections.defaultdict(list)
    for line in tsv(DB / "x1.tsv"):
        readers[line["engine"]].append(line)
    for engine, run in runs.items():
        queries = readers[engine]
        ok = [q for q in queries if q["status"] == "ok"]
        errors = collections.Counter(q["error"][:80] for q in queries if q["status"] == "error")
        batches = writer[engine]
        metrics = {
            "writer_wall_s": run["writer_wall_s"], "writer_exit": run["writer_exit"],
            "writer_batches_ok": sum(1 for b in batches if b.get("error", "") == "" and b["batch"] != "writer_exit"),
            "writer_batches_error": sum(1 for b in batches if b.get("error", "")),
            "writer_open_retries": sum(int(b["open_retries"] or 0) for b in batches if b["batch"] != "writer_exit"),
            "final_count": run["final_count"], "reader_queries": len(queries), "reader_queries_ok": len(ok),
            "reader_queries_error": len(queries) - len(ok), "reader_inconsistent": sum(q["consistent"] == "no" for q in ok),
            "reader_latency_median_s": median(q["latency_s"] for q in queries),
            "reader_latency_max_s": f"{max((float(q['latency_s']) for q in queries), default=0):.3f}",
            "reader_distinct_counts_seen": len({q["count"] for q in ok}),
        }
        for metric, value in metrics.items():
            out.append(row(run="X1", case="append", corpus="synthetic", engine=engine.split("_")[0], variant=engine,
                           load_average_1m=run["load_average_1m"], metric=metric, value=value, status="ok"))
        for message, count in errors.most_common(3):
            out.append(row(run="X1", case="append", corpus="synthetic", engine=engine.split("_")[0], variant=engine,
                           metric="reader_error", value=count, note=message, status="error"))
    return out


def s1_rows():
    return [row(run="S1", case="ancestor", corpus=line["corpus"], engine=line["format"].split("_")[0] if not line["format"].startswith("w1_") else line["format"][3:],
                variant=line["format"], metric="bytes", value=line["bytes"], status="ok") for line in tsv(DB / "s1.tsv")]


def b1_rows():
    groups = collections.defaultdict(list)
    for line in tsv(DB / "b1.tsv"):
        groups[(line["variant"], line["step"])].append(line)
    out = []
    for (variant, step), lines in groups.items():
        out.append(row(run="B1", case="lab_writer", corpus="", engine="duckdb" if variant == "duck" else "none", variant=step,
                       repeats=len(lines), load_average_1m=lines[0]["load_average_1m"], wall_s=median(l["wall_s"] for l in lines),
                       metric="binary_bytes", value=lines[-1]["binary_bytes"], status="ok"))
    return out


def grid(rows):
    cols = COLUMNS
    html = f"""<!doctype html><html><head><meta charset="utf-8"><title>lab-20261003-duckdb-vs-sqlite-post-store</title>
<link rel="stylesheet" href="https://cdn.datatables.net/2.1.8/css/dataTables.dataTables.min.css">
<style>body{{font:14px system-ui;margin:16px}} tfoot input,tfoot select{{width:100%}}</style>
<script src="https://code.jquery.com/jquery-3.7.1.min.js"></script>
<script src="https://cdn.datatables.net/2.1.8/js/dataTables.min.js"></script></head><body>
<h3>DuckDB vs SQLite post-store: one row per run / case / corpus / engine / variant (medians of repeats); filter run = Q1 for the per-case table</h3>
<table id="g" class="display compact" style="width:100%"><thead><tr>{''.join(f'<th>{c}</th>' for c in cols)}</tr></thead>
<tfoot><tr>{''.join(f'<th>{c}</th>' for c in cols)}</tr></tfoot></table>
<script>
const data = {json.dumps(rows)};
const cols = {json.dumps(cols)};
$(function() {{
  $('#g').DataTable({{
    data, pageLength: 100,
    columns: cols.map(c => ({{data: c, title: c, defaultContent: ''}})),
    initComplete: function() {{
      this.api().columns().every(function() {{
        const col = this, vals = [...new Set(col.data().toArray())].sort();
        if (vals.length <= 30) {{
          const s = $('<select><option value="">all</option></select>').appendTo($(col.footer()).empty())
            .on('change', function() {{ col.search(this.value ? '^' + $.fn.dataTable.util.escapeRegex(this.value) + '$' : '', true, false).draw(); }});
          vals.forEach(v => s.append(`<option>${{v}}</option>`));
        }} else {{
          $('<input placeholder="filter">').appendTo($(col.footer()).empty()).on('keyup change', function() {{ col.search(this.value).draw(); }});
        }}
      }});
    }}
  }});
}});
</script></body></html>"""
    (HERE / "grid.html").write_text(html)


def main():
    results = measured()
    extra = w1_rows(results) + c1_rows() + x1_rows() + s1_rows() + b1_rows()
    rows = results + extra
    with (HERE / "results.tsv").open("w") as handle:
        handle.write("\t".join(COLUMNS) + "\n")
        for result in rows:
            handle.write("\t".join(result[c].replace("\t", " ").replace("\n", " ") for c in COLUMNS) + "\n")
    grid(rows)
    print(len(rows), "rows -> results.tsv, grid.html")


if __name__ == "__main__":
    main()
