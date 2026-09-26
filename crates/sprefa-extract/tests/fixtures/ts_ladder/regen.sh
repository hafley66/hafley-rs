#!/usr/bin/env bash
set -euo pipefail
here=tests/fixtures/ts_ladder
scip-typescript index --cwd "$here" --output index.scip --no-progress-bar
