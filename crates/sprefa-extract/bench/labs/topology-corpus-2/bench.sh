#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 2 ]]; then echo "usage: $0 RYI_BINARY fast|slow|oracle" >&2; exit 2; fi
exec python3 "$(dirname "$0")/scratch/bench.py" "$1" "$2"
