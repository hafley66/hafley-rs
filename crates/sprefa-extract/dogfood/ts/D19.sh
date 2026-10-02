#!/bin/bash
set -euo pipefail
"$RYII" diff --from HEAD~1 --to HEAD --pattern '**/*.ts' --pattern '**/*.tsx' > "$STATE/D19-corpus.jsonl"
jq -se 'all(.[]; .callee_name as $name | .record != "diff_edge" or (.caller_path | endswith("/6_graphRenderer.ts") | not) or (["sameCamera","hoverOpacity","graphHoverColor"] | index($name)) == null)' "$STATE/D19-corpus.jsonl" >/dev/null
repo="$STATE/D19-repo"
mkdir -p "$repo"
git -C "$repo" init -q
git -C "$repo" config user.name 'Dogfood fixture'
git -C "$repo" config user.email 'dogfood@example.invalid'
cat > "$repo/0_calls.ts" <<'TS'
export function helper(){return 1}
export function outer(){return [()=>helper(),()=>helper()]}
TS
git -C "$repo" add 0_calls.ts
git -C "$repo" -c core.hooksPath=/dev/null commit -qm before
"$RYII" --resolve "$repo/0_calls.ts" > "$STATE/D19-before.jsonl"
cp "$repo/0_calls.ts" "$STATE/D19-source.ts"
{ printf '// unrelated prefix\nconst unrelated=[()=>2]\n'; cat "$STATE/D19-source.ts"; } > "$repo/0_calls.ts"
git -C "$repo" add 0_calls.ts
git -C "$repo" -c core.hooksPath=/dev/null commit -qm after
"$RYII" --resolve "$repo/0_calls.ts" > "$STATE/D19-after.jsonl"
for side in before after; do
  jq -s '[.[] | select(.record=="resolved_edge" and (.caller_name | startswith("closure@"))) | {caller_name,callee_name,kind,resolution_origin}] | sort' "$STATE/D19-$side.jsonl" > "$STATE/D19-$side.keys"
done
cmp "$STATE/D19-before.keys" "$STATE/D19-after.keys"
jq -e 'length==2 and (map(.caller_name)|unique|length)==2' "$STATE/D19-after.keys" >/dev/null
"$RYII" diff --root "$repo" --from HEAD~1 --to HEAD --pattern '**/*.ts' > "$STATE/D19-repro.jsonl"
jq -se 'all(.[]; .record != "diff_edge")' "$STATE/D19-repro.jsonl" >/dev/null
echo 'D19: unchanged closure fact identities survive prefix/sibling edits; duplicate bodies remain distinct'
