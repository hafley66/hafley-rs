#!/usr/bin/env bash

# Crate name and the bare verb `extract` (1228 hits / 279 files) stay. Idempotent.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
C=crates/sprefa-extract

step() { printf '\n== %s\n' "$1"; }

step "1/5 move the bin entrypoint"
[ -f "$C/src/bin/extract.rs" ] && git mv "$C/src/bin/extract.rs" "$C/src/bin/ryi.rs"

step "2/5 move the bin module dir"
[ -d "$C/src/bin/extract" ] && git mv "$C/src/bin/extract" "$C/src/bin/ryi"

step "3/5 Cargo.toml [[bin]] target"
sed -i '' \
  -e 's|^name = "extract"$|name = "ryi"|' \
  -e 's|^path = "src/bin/extract\.rs"$|path = "src/bin/ryi.rs"|' \
  "$C/Cargo.toml"
grep -n -A3 '\[\[bin\]\]' "$C/Cargo.toml"

step "4/5 test harness bin handle (183 hits)"
git grep -l 'CARGO_BIN_EXE_extract' -- "$C" \
  | xargs sed -i '' 's/CARGO_BIN_EXE_extract/CARGO_BIN_EXE_ryi/g'

step "5/5 --bin extract in docs and schema"
# TASKS/*.BRIEF.md and kimi.turns.json are frozen records, out of scope.
git grep -l -- '--bin extract' -- "$C" \
  | xargs -r sed -i '' 's/--bin extract/--bin ryi/g'

step "residue check (expect zero)"
git grep -n 'CARGO_BIN_EXE_extract' -- "$C" && exit 1
git grep -n -- '--bin extract' -- "$C" && exit 1
echo "clean"

step "gate"
echo "cargo test -p sprefa-extract --features cli"
