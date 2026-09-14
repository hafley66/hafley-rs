#!/usr/bin/env bash
# Build the whole docs site into one directory: the mdBook at the root and the
# rustdoc for every workspace crate under api/. Also writes the API landing
# index and runs the coverage/link checker.
#
# Usage: scripts/docs/0_build_site.sh [site-dir]
#   site-dir defaults to <repo>/target/docs-site. Relative paths are resolved
#   against the repository root.
#
# WARNING: this does a full workspace `cargo doc` and wipes site-dir first.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
site_dir="${1:-target/docs-site}"
case "$site_dir" in
  /*) ;;
  *) site_dir="$repo_root/$site_dir" ;;
esac

command -v mdbook >/dev/null || { echo "mdbook is not on PATH (pin 0.5.4)" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo is not on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is not on PATH" >&2; exit 1; }

rm -rf "$site_dir"
mkdir -p "$site_dir"

echo "docs: building book -> $site_dir"
mdbook build "$repo_root/docs/book" --dest-dir "$site_dir"
# rustdoc ships files under src/ and other names Jekyll must not touch; the
# official Pages artifact does not run Jekyll, but this keeps local serving and
# any static host honest.
touch "$site_dir/.nojekyll"

echo "docs: building workspace rustdoc"
cargo doc --workspace --no-deps --locked --manifest-path "$repo_root/Cargo.toml"

target_dir="$(cargo metadata --manifest-path "$repo_root/Cargo.toml" \
  --no-deps --format-version 1 --locked \
  | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"

echo "docs: copying $target_dir/doc -> $site_dir/api"
rm -rf "$site_dir/api"
cp -R "$target_dir/doc" "$site_dir/api"

echo "docs: writing api/index.html"
python3 "$repo_root/scripts/docs/1_index_api.py" \
  --repo-root "$repo_root" \
  --doc-dir "$target_dir/doc" \
  --out "$site_dir/api/index.html"

echo "docs: checking coverage and links"
base_path="$(python3 -c 'import sys, tomllib; c = tomllib.load(open(sys.argv[1], "rb")); print(c["output"]["html"].get("site-url", "/"))' "$repo_root/docs/book/book.toml")"
python3 "$repo_root/scripts/docs/2_check_site.py" \
  --repo-root "$repo_root" \
  --site-dir "$site_dir" \
  --doc-dir "$target_dir/doc" \
  --base-path "$base_path"

echo "docs: site ready at $site_dir"
