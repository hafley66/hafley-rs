#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -x "$1" ]]; then
  echo "usage: $0 <executable-ryii>" >&2
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
crate_dir=$(cd -- "$script_dir/.." && pwd)
repo_root=$(cd -- "$crate_dir/../.." && pwd)
ryii=$1
if [[ "$ryii" != /* ]]; then
  ryii=$(cd -- "$(dirname -- "$ryii")" && pwd)/$(basename -- "$ryii")
fi
roots=(crates/hafley_scm/src crates/sprefa-extract/src crates/ryi/src crates/ryi-proto/src)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failed=0

while IFS= read -r query_file; do
  id=$(basename -- "$query_file" | cut -d_ -f1)
  raw="$tmp/$id.raw"
  hits="$tmp/$id.hits"
  fresh="$tmp/$id.fresh"
  query=$(cat -- "$query_file")

  (cd "$repo_root" && "$ryii" query --pattern '*.rs' --query "$query" "${roots[@]}") >"$raw"
  python3 - "$raw" >"$hits" <<'PY'
import json
import sys

for line in open(sys.argv[1], encoding="utf-8"):
    row = json.loads(line)
    print(f"{row['path']}:{row['line']}")
PY
  sort -u "$hits" -o "$hits"

  allow="$crate_dir/gate/allow/$id.txt"
  if [[ ! -f "$allow" ]]; then
    echo "missing allowlist: $allow" >&2
    exit 2
  fi
  sort -u "$allow" >"$tmp/$id.allow"
  comm -23 "$hits" "$tmp/$id.allow" >"$fresh"
  if [[ -s "$fresh" ]]; then
    while IFS= read -r hit; do
      printf '%s [%s]\n' "$hit" "$id"
    done <"$fresh"
    failed=1
  fi
done < <(find "$crate_dir/gate" -maxdepth 1 -type f -name 'S*.scm' -print | sort)

exit "$failed"
