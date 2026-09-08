#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
if [ "${1:-check}" = fetch ]; then
  for entry in 6:JumpSquat 7:Fall 8:LandingAirF 9:LandingHeavy; do
    number=${entry%%:*}
    action=${entry#*:}
    file="$lab_dir/../fixtures/falcon/${number}_pm36_${action}.html"
    if [ ! -f "$file" ]; then
      download=$(mktemp)
      curl --fail --location --silent --show-error --compressed --max-time 45 \
        "https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/${action}.html" -o "$download"
      mv "$download" "$file"
    fi
  done
fi
node "$lab_dir/97a_sources.mjs"
lab_cargo run --bin falcon-import
