#!/usr/bin/env bash
# Compatibility entry. The central gate owns isolated fixtures and assertions.
set -euo pipefail
exec bash "$(dirname "$0")/0_regression_gate.sh" live "$@"
