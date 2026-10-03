#!/usr/bin/env bash
# usage: 6_gritql.sh [small|crates ...]  -> arm G: grit apply --dry-run --jsonl; G = one process over the corpus,
# G_per_file = one process per file (a file whose grit run dies is recorded in the note and contributes no rows)
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$here/0_measure.sh"
repo=$(cd -- "$here/../../../../.." && pwd)
grit=${GRIT:-$here/db/bin/grit}
mkdir -p "$here/db/grit-cwd"
cd "$here/db/grit-cwd"
to_tsv() { # TASK FILE: grit jsonl -> TSV in place
  python3 - "$1" "$2" "$repo" <<'PY'
import json, sys, os
task, path, repo = sys.argv[1], sys.argv[2], sys.argv[3]
rows = set()
for line in open(path, errors="replace"):
    try:
        match = json.loads(line)
    except ValueError:
        continue
    if match.get("__typename") != "Match":
        continue
    source = os.path.relpath(os.path.join(repo, match["sourceFile"]) if not match["sourceFile"].startswith("/") else match["sourceFile"], repo)
    if task == "T2":
        rows |= {(source, r["startByte"], r["endByte"]) for r in match["ranges"]}
    else:
        rows |= {(source, r["startByte"], r["endByte"]) for v in match["variables"] if v["name"] == "$link" for r in v["ranges"]}
header = ("path", "fn_start", "fn_end") if task == "T2" else ("path", "link_start", "link_end")
open(path, "w").write("\t".join(header) + "\n" + "".join("\t".join(map(str, r)) + "\n" for r in sorted(rows)))
PY
}
for corpus in ${@:-small crates}; do
  case $corpus in
    small) inputs=("$repo/crates/hafley_scm/src" "$repo/crates/sprefa-extract/src") ;;
    crates) inputs=("$repo/crates") ;;
  esac
  for task in T2 T1; do
    pattern=$here/6_gritql/$([[ $task == T2 ]] && echo self_call || echo chain).grit
    lines=$(grep -c -v -E '^\s*(//.*)?$' "$pattern")
    measure G "$task" "$corpus" cst 1 0 "$lines" "G_${task}_${corpus}_cst" -- \
      "$grit" apply --dry-run --jsonl "$pattern" "${inputs[@]}"
    to_tsv "$task" "$here/db/out/G_${task}_${corpus}_cst.tsv"
    files=$here/db/grit-files-$corpus.txt
    (cd "$repo" && git ls-files -- "${inputs[@]/#$repo\//}" | grep '\.rs$' | sed "s|^|$repo/|") > "$files"
    measure G_per_file "$task" "$corpus" cst 1 0 "$lines" "G_per_file_${task}_${corpus}_cst" -- \
      bash -c 'while read -r f; do "$0" apply --dry-run --jsonl "$1" "$f" 2>/dev/null || echo "{\"__typename\":\"Died\",\"file\":\"$f\"}"; done < "$2"' \
      "$grit" "$pattern" "$files"
    died=$(grep -c '"Died"' "$here/db/out/G_per_file_${task}_${corpus}_cst.tsv" || true)
    to_tsv "$task" "$here/db/out/G_per_file_${task}_${corpus}_cst.tsv"
    python3 - "$here/db/runs.tsv" "G_per_file_${task}_${corpus}_cst" "$died" <<'PY'
import sys
path, out, died = sys.argv[1:]
lines = open(path).read().splitlines()
head = lines[0].split("\t")
rows = [line.split("\t") for line in lines[1:]]
row = next(r for r in reversed(rows) if r[head.index("output")] == out)
row[head.index("note")] = f"{died} files: grit process died (stack overflow); no rows from them"
open(path, "w").write("\n".join([lines[0]] + ["\t".join(r) for r in rows]) + "\n")
PY
  done
done
