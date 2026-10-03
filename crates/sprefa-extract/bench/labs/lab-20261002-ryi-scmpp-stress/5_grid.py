"""Lab tables -> plans/2026-10-02-ryi-graph-scmpp-stress-grid.html (jQuery DataTables, data inlined; bench_grid.py pattern).

Tables: graph fast vs slow agreement (results.db agree), scm++ stress (stress.tsv), scm++ oracle (oracle.tsv).
"""
import csv, json, sqlite3, pathlib

HERE = pathlib.Path(__file__).resolve().parent
WT = HERE.parents[4]
GRAPH = HERE.parent / "lab-20261002-ryi-graph-fast-vs-slow" / "results.db"
OUT = WT / "plans" / "2026-10-02-ryi-graph-scmpp-stress-grid.html"


def tsv(path):
    with open(path) as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    for row in rows:
        for key, value in row.items():
            try:
                row[key] = float(value) if "." in value else int(value)
            except (TypeError, ValueError):
                pass
    return rows


db = sqlite3.connect(GRAPH)
db.row_factory = sqlite3.Row
tables = {
    "graph fast vs slow: one row per language / arm / anchor (edge-key sets)": [dict(r) for r in db.execute("select * from agree order by language, arm, category, anchor")],
    "scm++ stress: wall time, peak RSS, DB size, row counts": tsv(HERE / "stress.tsv"),
    "scm++ oracle: single-relation cases vs Python tree walk over the run's CST rows": tsv(HERE / "oracle.tsv"),
}
parts = []
for index, (title, rows) in enumerate(tables.items()):
    cols = list(rows[0].keys())
    parts.append(f"""<h3>{title}</h3>
<table id="t{index}" class="display compact" style="width:100%"><thead><tr>{''.join(f'<th>{c}</th>' for c in cols)}</tr></thead>
<tfoot><tr>{''.join(f'<th>{c}</th>' for c in cols)}</tr></tfoot></table>
<script>grid('#t{index}', {json.dumps(rows)}, {json.dumps(cols)});</script>""")
html = f"""<!doctype html><html><head><meta charset="utf-8"><title>ryi graph + scm++ stress</title>
<link rel="stylesheet" href="https://cdn.datatables.net/2.1.8/css/dataTables.dataTables.min.css">
<style>body{{font:14px system-ui;margin:16px}} td.num{{text-align:right}} tfoot input,tfoot select{{width:100%}}</style>
<script src="https://code.jquery.com/jquery-3.7.1.min.js"></script>
<script src="https://cdn.datatables.net/2.1.8/js/dataTables.min.js"></script>
<script>
function grid(id, data, cols) {{
  $(function() {{
    $(id).DataTable({{
      data, pageLength: 50,
      columns: cols.map(c => ({{data: c, title: c, defaultContent: '', className: typeof data[0][c] === 'number' ? 'num' : ''}})),
      initComplete: function() {{
        this.api().columns().every(function() {{
          const col = this, vals = [...new Set(col.data().toArray())].filter(v => v !== null && v !== '').sort();
          if (vals.length <= 30 && typeof vals[0] === 'string') {{
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
}}
</script></head><body>
{''.join(parts)}
</body></html>"""
OUT.write_text(html)
print(OUT, {title: len(rows) for title, rows in tables.items()})
