# hafley-rs agent rules

## Tabular results
- Any table with more than ~10 rows or more than 5 columns goes to a browser grid, not chat:
  `python3 scripts/bench_grid.py` (jQuery DataTables, data inlined, per-column filters) writes
  `plans/bench-grid.html`; open it. Copy the script's pattern for other SQLite sources.
- Tables are 1NF: one value per cell, one column per metric. Never pack two metrics into one cell
  ("0.98 / 1.0", "2/2 sites; 1/1 files").
- Column names are full words (`file_recall`, `targets`), never single letters (R, P, n).

## Tool evaluation scope
- Rust and TypeScript only until ryi and its rivals are proven on those two. No Kotlin, Go or
  Python runs, fixtures, or truth indexes until the user lifts this.
