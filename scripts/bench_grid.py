"""Render bench.db scores or --json INPUT --out OUTPUT --title TITLE as a grid."""
import json, os, sqlite3, sys
from html import escape
BENCH = os.path.expanduser("~/.cache/lanes/claude-375/eval/bench")
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/projects/hafley-rs/plans/bench-grid.html")
LANG = {"tokio": "rust", "hafley-rs": "rust", "hafley_scm": "rust", "django": "python", "requests": "python",
        "graphify-src": "python", "vite": "typescript", "codegraph-src": "typescript", "gin": "go"}
TRUTH_ENGINE = {"rust": "rust-analyzer", "python": "scip-python", "typescript": "scip-typescript", "go": "scip-go"}
SAME = {("serena", "rust")}
TITLE = 'Correctness vs SCIP truth: targets averaged'
ORDER = [[6, 'desc']]
if '--json' in sys.argv:
    import argparse
    parser = argparse.ArgumentParser(description='Render existing benchmark rows as a filterable grid')
    parser.add_argument('--json', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--title', default='Benchmark results')
    args = parser.parse_args()
    with open(args.json) as source:
        rows = json.load(source)
    OUT, TITLE, ORDER = args.out, args.title, [[0, 'asc']]
else:
    db = sqlite3.connect(f"{BENCH}/bench.db")
    rows = []
    for tool, endpoint, repo, kind, n, fr, fp, sr, sp in db.execute("""
        select tool, endpoint, repo, kind, count(*), avg(file_r), avg(file_p), avg(site_r), avg(site_p)
        from score group by 1,2,3,4"""):
        lang = LANG.get(repo, "other")
        r = lambda x: None if x is None else round(x, 3)
        rows.append({"tool": tool, "endpoint": endpoint, "language": lang, "repo": repo, "kind": kind, "targets": n,
                     "file_recall": r(fr), "file_precision": r(fp), "site_recall": r(sr), "site_precision": r(sp),
                     "truth_engine": TRUTH_ENGINE.get(lang, ""), "same_engine_as_truth": "yes" if (tool, lang) in SAME else "no"})
cols = list(rows[0].keys()) if rows else []
html = f"""<!doctype html><html><head><meta charset="utf-8"><title>bench grid</title>
<link rel="stylesheet" href="https://cdn.datatables.net/2.1.8/css/dataTables.dataTables.min.css">
<style>body{{font:14px system-ui;margin:16px}} td.num{{text-align:right}} tfoot input,tfoot select{{width:100%}}</style>
<script src="https://code.jquery.com/jquery-3.7.1.min.js"></script>
<script src="https://cdn.datatables.net/2.1.8/js/dataTables.min.js"></script></head><body>
<h3>{escape(TITLE)}</h3>
<table id="g" class="display compact" style="width:100%"><thead><tr>{''.join(f'<th>{escape(c)}</th>' for c in cols)}</tr></thead>
<tfoot><tr>{''.join(f'<th>{escape(c)}</th>' for c in cols)}</tr></tfoot></table>
<script>
const data = {json.dumps(rows).replace(chr(60), chr(92) + "u003c")};
const cols = {json.dumps(cols)};
$(function() {{
  $('#g').DataTable({{
    data, pageLength: 100, order: {json.dumps(ORDER)},
    columns: cols.map(c => ({{data: c, title: c, render: $.fn.dataTable.render.text(), defaultContent: '', className: typeof data[0]?.[c] === 'number' || c.includes('recall') || c.includes('precision') ? 'num' : ''}})),
    initComplete: function() {{
      this.api().columns().every(function() {{
        const col = this, vals = [...new Set(col.data().toArray())].filter(v => v !== null).sort();
        if (vals.length <= 30 && typeof vals[0] === 'string') {{
          const s = $('<select><option value="">all</option></select>').appendTo($(col.footer()).empty())
            .on('change', function() {{ col.search(this.value ? '^' + $.fn.dataTable.util.escapeRegex(this.value) + '$' : '', true, false).draw(); }});
          vals.forEach(v => s.append($('<option>').val(v).text(v)));
        }} else {{
          $('<input placeholder="filter">').appendTo($(col.footer()).empty()).on('keyup change', function() {{ col.search(this.value).draw(); }});
        }}
      }});
    }}
  }});
}});
</script></body></html>"""
open(OUT, "w").write(html)
print(OUT, len(rows), "rows")
