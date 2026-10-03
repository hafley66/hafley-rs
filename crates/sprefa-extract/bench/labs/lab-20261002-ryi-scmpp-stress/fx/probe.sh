#!/bin/bash
# usage: probe.sh FILE... -- 'QUERY'  -> __text columns per row, stderr, exit code
R=/Users/chrishafley/projects/hafley-rs/.boop-worktrees/lab/ryi-stress/crates/sprefa-extract/target/release/ryii
S=/private/tmp/claude-501/-Users-chrishafley-projects/e589d7bf-2c40-4832-bab4-cbd66768ee58/scratchpad
files=(); while [ "$1" != "--" ]; do files+=("$1"); shift; done; shift
printf '%s\n' "$1" > $S/q.scm
RUST_LOG=warn $R query --scmpp $S/q.scm "${files[@]}" > $S/p.out 2> $S/p.err; code=$?
jq -c 'with_entries(select(.key == "path" or (.key | endswith("__text"))))' < $S/p.out 2>/dev/null || cat $S/p.out
cat $S/p.err | cut -c1-400
echo "exit=$code"
