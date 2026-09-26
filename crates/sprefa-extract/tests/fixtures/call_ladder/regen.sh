#!/usr/bin/env bash
set -euo pipefail
here=tests/fixtures/call_ladder
rust-analyzer scip "$here" --output "$here/index.scip"
rm -rf "$here/target" "$here/Cargo.lock"
