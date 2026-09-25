#!/usr/bin/env bash
# Rebuild the frozen soopy corpus and its SCIP index. Heavy: runs rust-analyzer.
# Run from crates/sprefa-extract.
set -euo pipefail
here=tests/fixtures/ratchet_soopy
soopy=../soopy
work=$(mktemp -d)

rm -rf "$here/src"
cp -R "$soopy/src" "$here/src"
rust-analyzer scip "$soopy" --output "$work/index.scip"
RATCHET_INDEX="$work/index.scip" cargo test --features cli --locked \
  --test 170_ratchet_sites_rust -- --ignored regen_fixture_index
rm -rf "$work"
