#!/usr/bin/env bash
# Install boop into ~/.cargo/bin from a commit that is already on origin/main,
# with that commit's sha stamped into `boop --version`.
#
# Three installs inside ten minutes on 2026-08-16 left nobody able to say which
# bytes a dying lane had run; the guard and the stamp exist for that.
#
# The install is rm, cp, codesign, in that order: a plain cp leaves the old
# macOS code signature attached to new bytes and the next run dies with
# "Killed: 9".
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"

git -C "$repo" fetch origin --quiet
bash "$here/install-guard.sh" "$repo"

sha="$(git -C "$repo" rev-parse --short HEAD)"
BOOP_BUILD_SHA="$sha" cargo build --release --manifest-path "$repo/Cargo.toml" -p boop

dest="${CARGO_HOME:-$HOME/.cargo}/bin/boop"
rm -f "$dest"
cp "$repo/target/release/boop" "$dest"
codesign --force --sign - "$dest"

echo "install-boop: installed $("$dest" --version) at $dest"
