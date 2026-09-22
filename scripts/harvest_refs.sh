#!/usr/bin/env bash
# harvest_refs.sh REPO PATTERN [GLOB]: grep every ref, PR and branch diffstat; read-only
set -euo pipefail
repo=${1:?repo}; pat=${2:?pattern}; glob=${3:-*.rs}
cd "$repo"
t() { timeout 10 "$@"; }

echo "== worktrees"; t git worktree list
echo "== prs"; t gh pr list --state all --limit 40 --json number,title,headRefName,state \
  --jq '.[] | "\(.number)\t\(.state)\t\(.headRefName)\t\(.title)"' 2>/dev/null || echo "gh unavailable"

echo "== branches not merged into main, diffstat filtered"
for b in $(t git for-each-ref --format='%(refname:short)' refs/heads refs/remotes | grep -v 'HEAD$'); do
  n=$(t git rev-list --count main.."$b" 2>/dev/null || echo 0)
  [ "$n" = 0 ] && continue
  echo "-- $b (+$n)"
  t git diff --stat main..."$b" 2>/dev/null | grep -iE "$pat" || true
done

echo "== hits across all refs (file:line, ref)"
t git grep -n -iE "$pat" $(t git for-each-ref --format='%(refname:short)' refs/heads refs/remotes | grep -v 'HEAD$') -- "$glob" \
  | awk -F: '{k=$2":"$3; if(!(k in s)){s[k]=$1; print $2":"$3"\t"$1}}' | sort -u

echo "== hit bodies (main or first ref that has it), 12 lines each"
t git grep -n -iE "$pat" $(t git for-each-ref --format='%(refname:short)' refs/heads refs/remotes | grep -v 'HEAD$') -- "$glob" \
  | awk -F: '{k=$2":"$3; if(!(k in s)){s[k]=1; print $1"\t"$2"\t"$3}}' \
  | while IFS=$'\t' read -r ref file line; do
      echo "--- $ref:$file:$line"
      t git show "$ref:$file" | sed -n "$((line>6?line-6:1)),$((line+6))p"
    done
