#!/usr/bin/env bash
# usage: scripts/ryi-e2e.sh [BIN_DIR]; ryi == ryii bytes and exit codes, then daemon lifecycle.
set -uo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../.." && pwd)
bin=${1:-${CARGO_TARGET_DIR:-$repo/target}/release}
ryi=$bin/ryi
ryii=$bin/ryii
for exe in "$ryi" "$ryii"; do
  [ -x "$exe" ] || { echo "missing $exe (cargo build --release -p ryi; cargo build --release --features cli --bin ryii in crates/sprefa-extract)" >&2; exit 2; }
done

# Short cache path: the unix socket path limit is ~100 bytes.
export XDG_CACHE_HOME
XDG_CACHE_HOME=$(mktemp -d /tmp/ryi-e2e.XXXXXX)
export RUST_LOG=off DL_TRAIL=0
sock=$XDG_CACHE_HOME/ryi/ryi.sock
out=$XDG_CACHE_HOME/out
mkdir -p "$out"

shutdown() {
  [ -S "$sock" ] && curl -s --unix-socket "$sock" -X POST http://ryi/__shutdown -o /dev/null
  local pid
  pid=$(cat "$XDG_CACHE_HOME/ryi/ryi.pid" 2>/dev/null) || return 0
  for _ in $(seq 1 50); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.1; done
  kill -9 "$pid" 2>/dev/null
}
trap 'shutdown; rm -rf "$XDG_CACHE_HOME"' EXIT

fail=0
row() { printf '%-6s %-44s %s\n' "$1" "$2" "$3"; [ "$1" = ok ] || fail=1; }

# same <label> <cwd> <stdin-file|-> <args...>: ryi and ryii byte-equal.
same() {
  local label=$1 cwd=$2 input=$3; shift 3
  local a=$out/a b=$out/b ae=$out/a.err be=$out/b.err ra rb
  if [ "$input" = - ]; then
    (cd "$cwd" && "$ryii" "$@" >"$a" 2>"$ae" </dev/null); ra=$?
    (cd "$cwd" && "$ryi" "$@" >"$b" 2>"$be" </dev/null); rb=$?
  else
    (cd "$cwd" && "$ryii" "$@" >"$a" 2>"$ae" <"$input"); ra=$?
    (cd "$cwd" && "$ryi" "$@" >"$b" 2>"$be" <"$input"); rb=$?
  fi
  local na nb
  na=$(wc -l <"$a" | tr -d ' '); nb=$(wc -l <"$b" | tr -d ' ')
  if [ "$ra" = "$rb" ] && cmp -s "$a" "$b" && cmp -s "$ae" "$be"; then
    row ok "$label" "rc=$ra rows=$na"
  else
    row FAIL "$label" "ryii rc=$ra rows=$na | ryi rc=$rb rows=$nb; stdout or stderr differs"
    diff -u "$ae" "$be" | head -30 || true
  fi
}

soopy=$repo/crates/soopy
fx=$here/tests/fixtures
cp -R "$fx/type_ladder" "$out/type_ladder"
type_ladder=$out/type_ladder
paths=$out/paths
printf 'src/lib.rs\nsrc/_1a_path.rs\n' >"$paths"

echo "== ryi vs ryii ($bin)"
same "fast . (soopy)"                "$soopy" - fast .
same "fast src/lib.rs (soopy)"       "$soopy" - fast src/lib.rs
same "fast \$PWD (soopy)"            "$soopy" - fast "$soopy"
same "fast - path list on stdin"     "$soopy" "$paths" fast -
same "slow . (type_ladder)"          "$type_ladder" - slow .
same "graph --callers (call_ladder)" "$fx/call_ladder" - graph --callers new .
same "query (type_ladder)"           "$type_ladder" - query --query '(struct_item name: (type_identifier) @n)' src
same "schema"                        "$soopy" - schema
same "missing path exit code"        "$soopy" - fast does/not/exist.rs
same "fast . again (warm)"           "$soopy" - fast .

footprint_mb() { vmmap --summary "$(cat "$XDG_CACHE_HOME/ryi/ryi.pid")" | awk '/^Physical footprint:/ {gsub(/M/, "", $3); printf "%d", $3; exit}'; }
for _ in 1 2; do (cd "$soopy" && "$ryi" fast . >/dev/null 2>&1); done
base=$(footprint_mb)
for _ in $(seq 3 10); do (cd "$soopy" && "$ryi" fast . >/dev/null 2>&1); done
last=$(footprint_mb)
if [ "$((last * 4))" -le "$((base * 5))" ]; then
  row ok "daemon footprint after 10 warm calls <= 1.25x call 2" "${base}MB -> ${last}MB"
else
  row FAIL "daemon footprint after 10 warm calls <= 1.25x call 2" "${base}MB -> ${last}MB"
fi

echo "== lifecycle"
if [ -S "$sock" ] && [ -f "$XDG_CACHE_HOME/ryi/ryi.pid" ]; then
  row ok "daemon up after ryi calls" "pid=$(cat "$XDG_CACHE_HOME/ryi/ryi.pid")"
else
  row FAIL "daemon up after ryi calls" "no socket or pid file"
fi
shutdown
if [ ! -e "$sock" ] && [ ! -e "$XDG_CACHE_HOME/ryi/ryi.pid" ]; then
  row ok "/__shutdown removes socket and pid" ""
else
  row FAIL "/__shutdown removes socket and pid" "$(ls "$XDG_CACHE_HOME/ryi")"
fi

(cd "$soopy" && RYI_IDLE_SECS=2 "$ryi" fast src/lib.rs >/dev/null 2>&1)
pid=$(cat "$XDG_CACHE_HOME/ryi/ryi.pid" 2>/dev/null)
for _ in $(seq 1 60); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
  row ok "idle exit (RYI_IDLE_SECS=2)" "pid=$pid"
else
  row FAIL "idle exit (RYI_IDLE_SECS=2)" "pid=${pid:-none} still running"
fi

trace=$XDG_CACHE_HOME/trace.json
(cd "$soopy" && HAFLEY_TRACE=$trace "$ryi" fast src/lib.rs >/dev/null 2>&1 \
  && HAFLEY_TRACE=$trace "$ryi" fast src/_1a_path.rs >/dev/null 2>&1)
shutdown
spans=$(python3 - "$trace" <<'EOF'
import json, sys
text = open(sys.argv[1]).read().rstrip().rstrip(',')
events = json.loads(text if text.endswith(']') else text + ']')
ids = {e['args']['request_id'] for e in events
       if e.get('name') == 'daemon_request' and e.get('args', {}).get('verb') == 'fast'}
print(len(ids))
EOF
)
if [ "$spans" = 2 ]; then
  row ok "observe: one daemon_request span per call" "fast requests=$spans"
else
  row FAIL "observe: one daemon_request span per call" "fast requests=${spans:-unreadable}"
fi

exit $fail
