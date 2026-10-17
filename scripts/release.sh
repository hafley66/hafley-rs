#!/usr/bin/env bash
# Build one crate's binaries for macOS on this machine and publish a GitHub
# release with a shell installer. No GitHub Actions.
#
#   scripts/release.sh <crate-dir> [rev]      rev defaults to origin/main
#   DRY_RUN=1 scripts/release.sh <crate-dir>  build only: no tag, no upload
#
# The tag is <package>-v<version> from the crate's Cargo.toml at <rev>; bump the
# version first, a published tag is never rebuilt. The build runs in a throwaway
# worktree of <rev>, so uncommitted work in this checkout never ships.
set -euo pipefail

crate=${1:?usage: scripts/release.sh <crate-dir> [rev]}
rev=${2:-origin/main}
repo=hafley66/hafley-rs
targets=(aarch64-apple-darwin x86_64-apple-darwin)
# Homebrew's rustc sits first on PATH and has no x86_64 std; the rustup
# toolchain carries both targets.
toolchain=${RELEASE_TOOLCHAIN:-nightly}

root=$(git rev-parse --show-toplevel)
git -C "$root" fetch -q origin --tags
commit=$(git -C "$root" rev-parse --verify "$rev^{commit}")

work=$(mktemp -d)
cleanup() { git -C "$root" worktree remove --force "$work/src" 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
git -C "$root" worktree add -q --detach "$work/src" "$commit"
src=$work/src
[ -f "$src/$crate/Cargo.toml" ] || { echo "release: no $crate/Cargo.toml at $rev" >&2; exit 1; }

rustup target add --toolchain "$toolchain" "${targets[@]}" >/dev/null
export PATH="$(dirname "$(rustup +"$toolchain" which rustc)"):$PATH"
export RCARGO_OFF=1

read -r name version < <(cargo metadata --no-deps --format-version 1 \
  --manifest-path "$src/$crate/Cargo.toml" \
  | python3 -c 'import json,sys,os; m=json.load(sys.stdin); want=os.path.realpath(sys.argv[1]); p=next(p for p in m["packages"] if os.path.realpath(p["manifest_path"])==want); print(p["name"], p["version"])' \
    "$src/$crate/Cargo.toml")
tag="$name-v$version"
echo "release: $tag from $(git -C "$root" log --oneline -1 "$commit")"

if [ -z "${DRY_RUN:-}" ]; then
  if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
    echo "release: $tag is already published; bump $crate's version" >&2; exit 1
  fi
  if existing=$(git -C "$root" rev-parse -q --verify "refs/tags/$tag^{commit}"); then
    [ "$existing" = "$commit" ] || { echo "release: tag $tag points at $existing, not $commit" >&2; exit 1; }
  else
    git -C "$root" tag -a "$tag" -m "$name v$version" "$commit"
    git -C "$root" push -q origin "$tag"
  fi
fi

# The dist workspace is the crate's own when it has one (crates excluded from
# the root workspace), else the repository root.
dist_dir=$src
[ -f "$src/$crate/dist-workspace.toml" ] && dist_dir=$src/$crate
target_args=(); for t in "${targets[@]}"; do target_args+=(--target "$t"); done
(cd "$dist_dir" && dist build --tag="$tag" --artifacts=all "${target_args[@]}")
out=$dist_dir/target/distrib
assets=("$out"/*-installer.sh "$out"/*.tar.xz "$out"/*.tar.xz.sha256 "$out"/sha256.sum "$out"/source.tar.gz "$out"/source.tar.gz.sha256)
ls -1 "${assets[@]}"

if [ -n "${DRY_RUN:-}" ]; then
  keep=$root/target/release-dry-run/$tag
  mkdir -p "$keep" && cp "${assets[@]}" "$keep/"
  echo "release: dry run, artifacts in $keep"
  exit 0
fi

installer=$(basename "$out"/*-installer.sh)
url="https://github.com/$repo/releases/download/$tag/$installer"
gh release create "$tag" --repo "$repo" --verify-tag --latest=false \
  --title "$name v$version (macOS)" \
  --notes "macOS, Apple Silicon and Intel. Built locally from $(git -C "$root" rev-parse --short "$commit"). Ad-hoc signed, not notarized.

\`\`\`sh
curl --proto '=https' --tlsv1.2 -LsSf $url | sh
\`\`\`" "${assets[@]}"

# Install from the public URL into a scratch CARGO_HOME; shell rc files untouched.
home=$work/install
curl --proto '=https' --tlsv1.2 -LsSf "$url" | CARGO_HOME=$home sh -s -- --no-modify-path >/dev/null
for bin in "$home"/bin/*; do "$bin" --version 2>/dev/null | head -1 || echo "$(basename "$bin") installed"; done
echo "release: $tag published"
