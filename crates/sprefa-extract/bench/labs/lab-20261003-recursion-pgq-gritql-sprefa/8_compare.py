"""Join every arm run (db/runs.tsv, 7_sprefa/runs.tsv) with the oracles -> results.tsv + grid.html; diffs -> db/diff/.

usage: python3 8_compare.py
"""
import csv, json, pathlib

HERE = pathlib.Path(__file__).resolve().parent
DB = HERE / "db"
COLUMNS = ["arm", "task", "corpus", "tier", "threads", "rows", "oracle_rows", "agree", "arm_only", "oracle_only",
           "wall_s", "peak_rss_mb", "setup_s", "notation_lines", "status", "note"]


def table(path):
    with open(path, newline="") as handle:
        lines = [line.rstrip("\n").split("\t") for line in handle if line.strip()]
    return (lines[0], lines[1:]) if lines else ([], [])


def oracle_path(task, corpus, tier):
    return DB / "oracle" / (f"{task}_{corpus}.tsv" if task in ("T1", "T2") else f"{task}_{corpus}_{tier}.tsv")


def score(run, out_dir):
    out = out_dir / f"{run['output']}.tsv"
    oracle = oracle_path(run["task"], run["corpus"], run["tier"])
    if run["status"] != "ok" or not out.exists() or not oracle.exists():
        return {}
    header, rows = table(out)
    oracle_header, oracle_rows = table(oracle)
    keep = [oracle_header.index(column) for column in header]
    got = {tuple(row) for row in rows if len(row) == len(header)}
    want = {tuple(row[i] for i in keep) for row in oracle_rows}
    arm_only, oracle_only = sorted(got - want), sorted(want - got)
    if arm_only or oracle_only:
        (DB / "diff").mkdir(exist_ok=True)
        (DB / "diff" / f"{run['output']}.txt").write_text(
            "columns\t" + "\t".join(header) + "\n" + "".join(f"arm_only\t{'	'.join(r)}\n" for r in arm_only[:20])
            + "".join(f"oracle_only\t{'	'.join(r)}\n" for r in oracle_only[:20]))
    return {"rows": len(got), "oracle_rows": len(want), "agree": len(got & want), "arm_only": len(arm_only),
            "oracle_only": len(oracle_only)}


def main():
    results = []
    for runs, out_dir in ((DB / "runs.tsv", DB / "out"), (HERE / "7_sprefa" / "runs.tsv", HERE / "7_sprefa" / "out")):
        if not runs.exists():
            continue
        with open(runs, newline="") as handle:
            latest = {run["output"]: run for run in csv.DictReader(handle, delimiter="\t")}
            for run in latest.values():
                row = {column: run.get(column, "") for column in COLUMNS}
                row.update(score(run, out_dir))
                results.append(row)
    with open(HERE / "results.tsv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, COLUMNS, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, escapechar="\\")
        writer.writeheader()
        writer.writerows(results)
    grid(results)
    print(len(results), "rows")


def grid(rows):
    html = f"""<!doctype html><html><head><meta charset="utf-8"><title>recursion lab</title>
<link rel="stylesheet" href="https://cdn.datatables.net/2.1.8/css/dataTables.dataTables.min.css">
<style>body{{font:14px system-ui;margin:16px}} tfoot input,tfoot select{{width:100%}}</style>
<script src="https://code.jquery.com/jquery-3.7.1.min.js"></script>
<script src="https://cdn.datatables.net/2.1.8/js/dataTables.min.js"></script></head><body>
<h3>lab-20261003-recursion-pgq-gritql-sprefa: one row per arm / task / corpus / tier / threads</h3>
<table id="g" class="display compact" style="width:100%"><thead><tr>{''.join(f'<th>{c}</th>' for c in COLUMNS)}</tr></thead>
<tfoot><tr>{''.join(f'<th>{c}</th>' for c in COLUMNS)}</tr></tfoot></table>
<script>
const data = {json.dumps(rows)};
const cols = {json.dumps(COLUMNS)};
$(function() {{
  $('#g').DataTable({{
    data, pageLength: 200, order: [[1, 'asc'], [2, 'asc'], [3, 'asc'], [0, 'asc']],
    columns: cols.map(c => ({{data: c, title: c, defaultContent: ''}})),
    initComplete: function() {{
      this.api().columns().every(function() {{
        const col = this, vals = [...new Set(col.data().toArray())].filter(v => v !== null && v !== '').sort();
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


if __name__ == "__main__":
    main()
