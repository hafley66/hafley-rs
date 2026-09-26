#!/usr/bin/env bash
# Run from crates/sprefa-extract after editing the ladder's src/.
set -euo pipefail
here=tests/fixtures/type_ladder
rust-analyzer scip "$here" --output "$here/index.scip"
rm -rf "$here/target" "$here/Cargo.lock"
