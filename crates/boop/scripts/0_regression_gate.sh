#!/usr/bin/env bash
# One entry point for deterministic assurance and opt-in authenticated trials.
set -euo pipefail
cd "$(dirname "$0")/../../.."
mode=${1:-deterministic}
case "$mode" in
  deterministic)
    cargo test --locked -p boop-store -p boop-harness -p boop-acp -p boop-proc -p boop-mux -p boop-turnvis -p boop
    cargo check --locked -p boop --no-default-features
    ;;
  live)
    : "${BOOP_E2E_ROOT:?Set BOOP_E2E_ROOT to a new task-owned diagnostic directory}"
    shift
    if [ "$#" -eq 0 ]; then set -- claude codex opencode ccz; fi
    cargo test --locked -p boop --test main --no-run
    failed=0
    for entry in "$@"; do
      BOOP_E2E_ENTRY="$entry" cargo test --locked -p boop --test main \
        t4_lifecycle_gate::authenticated_matrix -- --exact --ignored --nocapture || failed=1
    done
    exit "$failed"
    ;;
  *)
    echo 'Usage: just boop-check [deterministic|live]; script live optionally accepts entry names' >&2
    exit 2
    ;;
esac
