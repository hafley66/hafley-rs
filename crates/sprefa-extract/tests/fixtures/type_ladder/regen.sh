#!/usr/bin/env bash
# Run from crates/sprefa-extract after editing the ladder's src/.
set -euo pipefail
here=tests/fixtures/type_ladder
rust-analyzer scip "$here" --output "$here/index.scip"
rm -rf "$here/target" "$here/Cargo.lock"
# CodeQL's answers for the ladder, read by tests/179_codeql_baseline.rs.
out=$(mktemp -d)
RYI_CODEQL_OUT=$out scripts/ryi-vs-codeql.sh "$here" rust >/dev/null
cp "$out/type.csv" "$out/call.csv" tests/fixtures/codeql_baseline/type_ladder/
rm -rf "$out"
