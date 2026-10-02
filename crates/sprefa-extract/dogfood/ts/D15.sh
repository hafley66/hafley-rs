#!/bin/bash
# The coordinator's cli-only gate binary exercises the missing-feature build.
set -euo pipefail
: "${RYII:?}" "${STATE:?}"
binary=${RYII_NO_TS_CHECKER:-${CARGO_TARGET_DIR:?}/debug/ryii}
[[ -x "$binary" ]] || { echo 'D15 requires RYII_NO_TS_CHECKER (built with cli, without ts-checker)'; exit 1; }
for root in packages/md packages; do
  for query in from call-path callers; do
    args=(graph --slow)
    case "$query" in
      from) args+=(--from markdownTableModel) ;;
      call-path) args+=(--call-path markdownTableModel) ;;
      callers) args+=(--callers render) ;;
    esac
    if RUST_LOG=off "$binary" "${args[@]}" "$root" >"$STATE/D15.out" 2>"$STATE/D15.err"; then
      echo "D15 unexpected success: $query $root"; exit 1
    fi
    rg -q 'ts-checker' "$STATE/D15.err"
    ! rg -q '"record":"graph_edge"' "$STATE/D15.out"
  done
done
echo 'D15 missing checker: non-zero exit naming ts-checker for package and corpus roots'
