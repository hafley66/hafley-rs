#!/usr/bin/env bash
# usage: 1_before_after.sh RYII LABEL >> before_after.tsv
# Two scm++ queries over two corpora of the main hafley-rs checkout; --sqlite store, /usr/bin/time -l.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ryii=$1 label=$2
repo=${REPO:-/Users/chrishafley/projects/hafley-rs}
mkdir -p "$here/db"
printf '%s\n' '((call_expression) @call (#has-ancestor? @call function_item))' > "$here/db/ancestor.scm"
printf '%s\n' '((call_expression) @call)' > "$here/db/plain.scm"
for query in ancestor plain; do
  for corpus in crates scm_extract_src; do
    case $corpus in
      crates) inputs=("$repo/crates") ;;
      scm_extract_src) inputs=("$repo/crates/hafley_scm/src" "$repo/crates/sprefa-extract/src") ;;
    esac
    db="$here/db/$label-$query-$corpus.db"
    rm -f "$db"
    /usr/bin/time -l "$ryii" query --scmpp "$here/db/$query.scm" --sqlite "$db" --pattern '*.rs' "${inputs[@]}" >/dev/null 2>"$here/db/time.txt"
    wall=$(awk '/ real /{print $1}' "$here/db/time.txt")
    rss=$(awk '/maximum resident set size/{printf "%.0f", $1/1048576}' "$here/db/time.txt")
    size=$(wc -c < "$db" | awk '{printf "%.1f", $1/1048576}')
    rows=$(sqlite3 "$db" 'select count(*) from scmpp_row')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$query" "$corpus" "$label" "$wall" "$rss" "$size" "$rows"
  done
done
