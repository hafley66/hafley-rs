#!/usr/bin/env bash
# Install the pinned tools the live harness needs. Boop owns both the version
# and the location; nothing here reads ../instant or a global harness config.
#
# llmock is the provider mock (OpenAI Responses / Chat Completions / Anthropic
# Messages). tui-test-rs, the real PTY and terminal emulator, is a cargo
# dev-dependency and needs no install step.
set -euo pipefail

tools="${BOOP_LIVE_TOOLS:-${XDG_CACHE_HOME:-$HOME/.cache}/boop/live-tools}"
llmock="$tools/llmock/bin/llmock"

if [ -x "$llmock" ] && [ "${BOOP_LIVE_REFRESH:-0}" != "1" ]; then
  echo "llmock: $llmock (already installed)"
  exit 0
fi

mkdir -p "$tools"
cargo install --root "$tools/llmock" --force \
  --git https://github.com/larsakerlund/llmock.git --tag v0.1.2 --locked llmock
echo "llmock: $llmock"
