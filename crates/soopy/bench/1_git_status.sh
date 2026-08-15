#!/usr/bin/env bash
set -euo pipefail

mode=${1:?mode required}
repo=${2:-}
temporary=

if [[ "$mode" == smoke ]]; then
  temporary=$(mktemp -d "${TMPDIR:-/tmp}/soopy_git_status.XXXXXX")
  trap 'rm -rf "$temporary"' EXIT
  repo=$temporary/repository
  mkdir -p "$repo"
  git -C "$repo" init -q
  git -C "$repo" config user.name soopy
  git -C "$repo" config user.email soopy@example.invalid
  mkdir -p "$repo/src"
  printf 'pub const STATUS: u8 = 1;\n' > "$repo/src/status.rs"
  git -C "$repo" add src/status.rs
  git -C "$repo" commit -qm fixture
elif [[ "$mode" != repo || -z "$repo" ]]; then
  printf 'usage: %s smoke | repo <repository>\n' "$0" >&2
  exit 2
fi

cargo run -q -p soopy -- --repo "$repo" status-metrics
