#!/usr/bin/env bash
# Run from crates/sprefa-extract after editing src/.
set -euo pipefail
here=tests/fixtures/rename_merge
rust-analyzer scip "$here" --output "$here/index.scip"
rm -rf "$here/target" "$here/Cargo.lock"
