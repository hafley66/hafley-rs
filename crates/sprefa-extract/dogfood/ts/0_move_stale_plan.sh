#!/bin/bash
set -euo pipefail
fixture=$(mktemp -d /tmp/ryi-move-stale.XXXXXX)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/src/util"
cat > "$fixture/Cargo.toml" <<'TOML'
[package]
name = "move-stale-probe"
version = "0.0.0"
edition = "2024"
TOML
cat > "$fixture/src/lib.rs" <<'RS'
pub mod a;
pub mod util;
pub mod touched;
RS
cat > "$fixture/src/a.rs" <<'RS'
use super::*;
pub(super) fn original_name() {}
RS
printf 'pub fn marker() {}\n' > "$fixture/src/touched.rs"
printf 'pub fn utility() {}\n' > "$fixture/src/util/mod.rs"
git -C "$fixture" init -q
git -C "$fixture" add .
"$RYII" move "$fixture/src/a.rs" "$fixture/src/util/a.rs" --root "$fixture" --state "$STATE" --relocate-mod > .dogfood/stale-relocate.plan
"$RYII" rename "$fixture/src/a.rs#original_name" renamed_item --root "$fixture" --state "$STATE" --commit > .dogfood/stale-rename.commit
# Also alter an untouched declaration's length, exercising changed edit offsets.
printf 'pub mod a;\npub mod util;\npub mod touched;\npub const EXTRA: u8 = 1;\n' > "$fixture/src/lib.rs"
cp "$fixture/src/a.rs" "$fixture/expected-a.txt"
"$RYII" move "$fixture/src/a.rs" "$fixture/src/util/a.rs" --root "$fixture" --state "$STATE" --commit > .dogfood/stale-move.commit
cmp "$fixture/expected-a.txt" "$fixture/src/util/a.rs"
rg 'renamed_item' "$fixture/src/util/a.rs"
rg 'pub mod touched;' "$fixture/src/lib.rs"
rg 'EXTRA' "$fixture/src/lib.rs"
! rg 'pub mod a;' "$fixture/src/util/mod.rs"
[ ! -e "$fixture/src/a.rs" ]
echo 'stale-plan: changed flags and source bytes cannot replay the earlier relocate plan'
