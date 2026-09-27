#!/usr/bin/env bash
set -euo pipefail

update_allowlists=0
if [[ $# -eq 2 && "$1" == "--update-allowlists" ]]; then
  update_allowlists=1
  shift
fi
if [[ $# -ne 1 || ! -x "$1" ]]; then
  echo "usage: $0 [--update-allowlists] <executable-ryii>" >&2
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
test_roots=()
while IFS= read -r test_root; do
  test_roots+=("${test_root#"$repo_root"/}")
done < <(find "$repo_root/crates" -mindepth 2 -maxdepth 2 -type d -name tests -print | sort)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failed=0
context_query='(function_item name: (identifier) @name) @hit'

while IFS= read -r rule_file; do
  id=$(basename -- "$rule_file" | cut -d_ -f1)
  ext=${rule_file##*.}
  raw="$tmp/$id.raw"
  defs="$tmp/$id.defs"
  current="$tmp/$id.current"
  allow="$crate_dir/gate/allow/$id.txt"
  case "$id" in
    S029) rule_roots=(crates/sprefa-extract/src/bin crates/sprefa-extract/src/edit) ;;
    S182|S183) rule_roots=("${test_roots[@]}") ;;
    *) rule_roots=("${roots[@]}") ;;
  esac

  if [[ "$ext" == scm ]]; then
    query=$(cat -- "$rule_file")
    (cd "$repo_root" && RUST_LOG=off "$ryii" query --pattern '*.rs' --query "$query" "${rule_roots[@]}") >"$raw"
    (cd "$repo_root" && RUST_LOG=off "$ryii" query --pattern '*.rs' --query "$context_query" "${rule_roots[@]}") >"$defs"
    python3 - "$id" "$raw" "$defs" >"$current" <<'PY'
import collections
import json
import sys

rule_id, raw_path, defs_path = sys.argv[1:]
definitions = collections.defaultdict(list)
for line in open(defs_path, encoding="utf-8"):
    row = json.loads(line)
    definitions[row["path"]].append((row["line"], row["end_line"], row["name"]))

counts = collections.Counter()
for line in open(raw_path, encoding="utf-8"):
    row = json.loads(line)
    path, hit_line = row["path"], row["line"]
    owners = [
        (end - start, -start, name)
        for start, end, name in definitions.get(path, ())
        if start <= hit_line <= end
    ]
    if owners:
        item = "fn::" + min(owners)[2]
    else:
        item = "item::<file-scope>"
    if rule_id == "S029" and item == "item::<file-scope>":
        continue
    counts[(path, item, rule_id)] += 1

for (path, item, rule_id), count in sorted(counts.items()):
    print(f"{path}\t{item}\t{rule_id}\t{count}")
PY
  else
    sqlite_path="$tmp/source.db"
    if [[ ! -e "$sqlite_path" ]]; then
      (cd "$repo_root" && RUST_LOG=off "$ryii" fast --sqlite "$sqlite_path" "${roots[@]}") >/dev/null
    fi
    sqlite3 -tabs -noheader "$sqlite_path" <"$rule_file" >"$raw"
    python3 - "$id" "$raw" >"$current" <<'PY'
import collections
import sys

rule_id, raw_path = sys.argv[1:]
counts = collections.Counter()
for line in open(raw_path, encoding="utf-8"):
    path, name, count = line.rstrip("\n").split("\t")
    counts[(path, "fn::" + name, rule_id)] += int(count)

for (path, item, rule_id), count in sorted(counts.items()):
    print(f"{path}\t{item}\t{rule_id}\t{count}")
PY
  fi

  if [[ ! -f "$allow" ]]; then
    echo "missing allowlist: $allow" >&2
    exit 2
  fi
  if [[ "$update_allowlists" == 1 ]]; then
    cp -- "$current" "$allow"
    continue
  fi
  python3 - "$allow" "$current" "$id" <<'PY' || failed=1
import collections
import sys

allow_path, current_path, rule_id = sys.argv[1:]
def read_counts(path):
    counts = {}
    for line in open(path, encoding="utf-8"):
        path, item, row_rule, count = line.rstrip("\n").split("\t")
        if row_rule != rule_id:
            raise SystemExit(f"{path}: expected rule {rule_id}, found {row_rule}")
        counts[(path, item, row_rule)] = int(count)
    return counts

baseline = read_counts(allow_path)
current = read_counts(current_path)
for key, count in sorted(current.items()):
    previous = baseline.get(key, 0)
    if count > previous:
        path, item, _ = key
        print(f"{path} [{item}] [{rule_id}] count {previous} -> {count}")
        sys.exit(1)
PY
done < <(find "$crate_dir/gate" -maxdepth 1 -type f \( -name 'S*.scm' -o -name 'S*.sql' \) -print | sort)

exit "$failed"
