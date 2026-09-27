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
  filename=$(basename -- "$rule_file")
  rule=${filename%.*}
  ext=${filename##*.}
  description=$(sed -E '1s/^[;#-]+ ledger: S[0-9]+ //' "$rule_file" | head -n 1)
  raw="$tmp/$rule.raw"
  current="$tmp/$rule.current"
  allow="$crate_dir/gate/allow/$rule.txt"
  case "$rule" in
    env_read_in_request_path) rule_roots=(crates/sprefa-extract/src/bin crates/sprefa-extract/src/edit) ;;
    wall_clock_assert_in_test|external_tool_in_test) rule_roots=("${test_roots[@]}") ;;
    *) rule_roots=("${roots[@]}") ;;
  esac

  if [[ "$ext" == scm ]]; then
    query=$(cat -- "$rule_file")
    (cd "$repo_root" && RUST_LOG=off "$ryii" query --pattern '*.rs' --query "$query" "${rule_roots[@]}") >"$raw"
    (cd "$repo_root" && RUST_LOG=off "$ryii" query --pattern '*.rs' --query "$context_query" "${rule_roots[@]}") >"$tmp/$rule.defs"
    python3 - "$raw" "$tmp/$rule.defs" >"$current" <<'PY'
import collections
import json
import sys

raw_path, defs_path = sys.argv[1:]
definitions = collections.defaultdict(list)
for line in open(defs_path, encoding="utf-8"):
    row = json.loads(line)
    definitions[row["path"]].append((row["line"], row["end_line"], row["name"]))

hits = collections.defaultdict(list)
for line in open(raw_path, encoding="utf-8"):
    row = json.loads(line)
    path, hit_line = row["path"], row["line"]
    owners = [
        (end - start, -start, name)
        for start, end, name in definitions.get(path, ())
        if start <= hit_line <= end
    ]
    item = "fn::" + min(owners)[2] if owners else "item::<file-scope>"
    hits[(path, item)].append(hit_line)

for (path, item), lines in sorted(hits.items()):
    for hit_line in sorted(lines):
        print(f"{path}\t{item}\t{hit_line}")
PY
  else
    sqlite_path="$tmp/source.db"
    if [[ ! -e "$sqlite_path" ]]; then
      (cd "$repo_root" && RUST_LOG=off "$ryii" fast --sqlite "$sqlite_path" "${roots[@]}") >/dev/null
    fi
    sqlite3 -tabs -noheader "$sqlite_path" <"$rule_file" >"$raw"
    python3 - "$raw" "$repo_root" >"$current" <<'PY'
import collections
import pathlib
import sys

raw_path, repo_root = sys.argv[1:]
hits = collections.defaultdict(list)
for line in open(raw_path, encoding="utf-8"):
    path, name, start = line.rstrip("\n").split("\t")
    source = pathlib.Path(path)
    if not source.is_absolute():
        source = pathlib.Path(repo_root, source)
    content = source.read_bytes()
    line_number = content[:int(start)].count(b"\n") + 1
    hits[(path, "fn::" + name)].append(line_number)

for (path, item), lines in sorted(hits.items()):
    for hit_line in sorted(lines):
        print(f"{path}\t{item}\t{hit_line}")
PY
  fi

  if [[ ! -f "$allow" ]]; then
    echo "missing allowlist: $allow" >&2
    exit 2
  fi
  if [[ "$update_allowlists" == 1 ]]; then
    python3 - "$current" >"$allow" <<'PY'
import collections
import sys
counts = collections.Counter()
for line in open(sys.argv[1], encoding="utf-8"):
    path, item, _line = line.rstrip("\n").split("\t")
    counts[(path, item)] += 1
for (path, item), count in sorted(counts.items()):
    print(f"{path}\t{item}\t{count}")
PY
    continue
  fi
  python3 - "$allow" "$current" "$rule" "$description" <<'PY' || failed=1
import collections
import sys

allow_path, current_path, rule, description = sys.argv[1:]
baseline = {}
for line in open(allow_path, encoding="utf-8"):
    if not line.strip():
        continue
    path, item, count = line.rstrip("\n").split("\t")
    baseline[(path, item)] = int(count)
current = collections.defaultdict(list)
for line in open(current_path, encoding="utf-8"):
    path, item, hit_line = line.rstrip("\n").split("\t")
    current[(path, item)].append(int(hit_line))

new_hits = []
for key, lines in current.items():
    lines.sort()
    new_hits.extend((key[0], hit_line) for hit_line in lines[baseline.get(key, 0):])
for path, hit_line in sorted(new_hits):
    print(f"{path}:{hit_line} {rule}: {description}")
if new_hits:
    sys.exit(1)
PY
done < <(find "$crate_dir/gate" -maxdepth 1 -type f \( -name '*.scm' -o -name '*.sql' \) -print | sort)

exit "$failed"
