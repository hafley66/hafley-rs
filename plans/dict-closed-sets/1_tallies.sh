#!/usr/bin/env bash
set -euo pipefail
proof=$(cd "$(dirname "$0")" && pwd -P)
cd "$proof"
sqlite3 :memory: <<'SQL'
.mode tabs
.import 1_columns.tsv columns
.import 3_counts_38.tsv before_counts
.import 3_counts_40.tsv after_counts
.import 4_values_38.tsv before_values
.import 4_values_40.tsv after_values
.headers on
.once 12_column_tallies.tsv
SELECT c."table" AS table_name, c.new_column AS column_name,
       (SELECT count(*) FROM before_values b WHERE b."table"=c."table" AND b."column"=c.new_column) AS value_bins,
       coalesce((SELECT sum(rows) FROM before_values b WHERE b."table"=c."table" AND b."column"=c.new_column),0) AS before_rows,
       coalesce((SELECT sum(rows) FROM after_values a WHERE a."table"=c."table" AND a."column"=c.new_column),0) AS after_rows,
       NOT EXISTS (SELECT decoded_value,rows FROM before_values b WHERE b."table"=c."table" AND b."column"=c.new_column EXCEPT SELECT decoded_value,rows FROM after_values a WHERE a."table"=c."table" AND a."column"=c.new_column)
       AND NOT EXISTS (SELECT decoded_value,rows FROM after_values a WHERE a."table"=c."table" AND a."column"=c.new_column EXCEPT SELECT decoded_value,rows FROM before_values b WHERE b."table"=c."table" AND b."column"=c.new_column) AS multiset_equal
FROM columns c;
.once 13_table_tallies.tsv
SELECT b."table" AS table_name,b.rows AS before_rows,a.rows AS after_rows,
       (SELECT count(*) FROM columns c WHERE c."table"=b."table") AS enum_columns,
       (SELECT count(*) FROM before_values v WHERE v."table"=b."table") AS value_bins,
       NOT EXISTS (SELECT "column",decoded_value,rows FROM before_values v WHERE v."table"=b."table" EXCEPT SELECT "column",decoded_value,rows FROM after_values v WHERE v."table"=b."table")
       AND NOT EXISTS (SELECT "column",decoded_value,rows FROM after_values v WHERE v."table"=b."table" EXCEPT SELECT "column",decoded_value,rows FROM before_values v WHERE v."table"=b."table") AS multiset_equal
FROM before_counts b JOIN after_counts a ON a."table"=b."table";
.once 14_dictionary_tallies.tsv
SELECT c.dictionary AS dictionary_name,count(DISTINCT v.decoded_value) AS referenced_values
FROM columns c LEFT JOIN before_values v ON v."table"=c."table" AND v."column"=c.new_column AND v.decoded_value<>'NULL'
GROUP BY c.dictionary ORDER BY c.dictionary;
SQL
{
cat <<'HTML'
<!doctype html><meta charset="utf-8"><title>Schema 40 migration tallies</title>
<link rel="stylesheet" href="https://cdn.datatables.net/2.1.8/css/dataTables.dataTables.min.css">
<style>body{font:14px system-ui;margin:32px}h2{margin-top:40px}th,td{text-align:left}</style>
<h1>Schema 38 → 39 → 40 migration tallies</h1>
<p>Rows and decoded value multisets compare the backup copy before and after migration. Value bins include NULL. Dictionary tallies count referenced non-NULL values.</p>
HTML
for file in 13_table_tallies.tsv 12_column_tallies.tsv 14_dictionary_tallies.tsv; do
  printf '<h2>%s</h2><table class="tallies"><thead><tr>' "$file"
  IFS=$'\t' read -r -a headings < "$file"
  for field in "${headings[@]}"; do printf '<th>%s</th>' "$field"; done
  printf '</tr></thead><tbody>\n'
  while IFS=$'\t' read -r -a row; do
    printf '<tr>'
    for field in "${row[@]}"; do printf '<td>%s</td>' "$field"; done
    printf '</tr>\n'
  done < <(tail -n +2 "$file")
  printf '</tbody></table>\n'
done
cat <<'HTML'
<script src="https://code.jquery.com/jquery-3.7.1.min.js"></script>
<script src="https://cdn.datatables.net/2.1.8/js/dataTables.min.js"></script>
<script>$('.tallies').DataTable({paging:false,initComplete:function(){this.api().columns().every(function(){const column=this;const input=document.createElement('input');input.placeholder='Filter';input.style.width='100%';column.header().appendChild(input);input.addEventListener('input',()=>column.search(input.value).draw());});}});</script>
HTML
} > 15_tallies.html
