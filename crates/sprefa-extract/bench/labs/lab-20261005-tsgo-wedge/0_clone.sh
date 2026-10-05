#!/usr/bin/env bash
# Clones TypeScript v7.0.2 (Go source under tsc/) into bench/repos and builds stock and patched tsgo into bench/repos/tsgo-bin.
set -euo pipefail
lab="$(cd "$(dirname "$0")" && pwd)"
bench="$(cd "$lab/../.." && pwd)"
repo="$bench/repos/typescript-go"
bin="$bench/repos/tsgo-bin"
[ -d "$repo/.git" ] || git clone --depth 1 --branch v7.0.2 https://github.com/microsoft/TypeScript.git "$repo"
test "$(git -C "$repo" rev-parse HEAD)" = 1e4744d68260a7cb91b62b12edc3f6a2187faaf1
mkdir -p "$bin"
git -C "$repo" checkout -- tsc
rm -f "$repo/tsc/internal/ls/ryibatch.go"
(cd "$repo/tsc" && go build -o "$bin/tsgo-base" ./cmd/tsgo)
# Both wedges share checker/exports.go and ls/ryibatch.go; b adds the LSP hook, c the API hook.
git -C "$repo" apply "$lab/2_wedges/b_lsp_batch.patch"
git -C "$repo" apply --include='tsc/internal/api/*' "$lab/2_wedges/c_api_batch.patch"
(cd "$repo/tsc" && go build -o "$bin/tsgo-wedge" ./cmd/tsgo)
