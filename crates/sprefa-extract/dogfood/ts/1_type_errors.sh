#!/bin/bash
# Source from a case; all checker execution is deferred to that case's run.
type_errors() {
  local pkg=$1 phase=$2 rc=0
  "${TSC:-./node_modules/.bin/tsc}" --pretty false --noEmit -p ".dogfood/tsconfig.$pkg.json" > ".dogfood/$CASE.$phase.$pkg.txt" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ] && ! rg -q 'error TS[0-9]+' ".dogfood/$CASE.$phase.$pkg.txt"; then
    cat ".dogfood/$CASE.$phase.$pkg.txt"; return 1
  fi
  # Preserve diagnostic messages and duplicate counts while ignoring line shifts.
  { rg 'error TS[0-9]+' ".dogfood/$CASE.$phase.$pkg.txt" || true; } | sed -E 's/\([0-9]+,[0-9]+\): error /: error /' | LC_ALL=C sort > ".dogfood/$CASE.$phase.$pkg.errors"
}

type_baseline() {
  for pkg in "$@"; do type_errors "$pkg" before; done
}

type_no_new() {
  for pkg in "$@"; do
    type_errors "$pkg" after
    LC_ALL=C comm -13 ".dogfood/$CASE.before.$pkg.errors" ".dogfood/$CASE.after.$pkg.errors" > ".dogfood/$CASE.new.$pkg.errors"
    if [ -s ".dogfood/$CASE.new.$pkg.errors" ]; then
      cat ".dogfood/$CASE.new.$pkg.errors"; return 1
    fi
  done
}
