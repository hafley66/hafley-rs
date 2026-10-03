# sourced by 2_-6_ scripts: measure ARM TASK CORPUS TIER THREADS SETUP_S NOTATION_LINES OUT -- command...
# stdout of the command -> db/out/OUT.tsv; one row appended to db/runs.tsv
here=${here:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)}
mkdir -p "$here/db/out"
[[ -f $here/db/runs.tsv ]] || printf 'arm\ttask\tcorpus\ttier\tthreads\twall_s\tpeak_rss_mb\tsetup_s\tnotation_lines\tstatus\tnote\toutput\n' > "$here/db/runs.tsv"
measure() {
  local arm=$1 task=$2 corpus=$3 tier=$4 threads=$5 setup=$6 lines=$7 out=$8; shift 9
  local status=ok note= code=0
  /usr/bin/time -l timeout "${TIMEOUT:-900}" "$@" > "$here/db/out/$out.tsv" 2> "$here/db/out/$out.err" || code=$?
  if [[ $code == 124 ]]; then status=error note="timeout ${TIMEOUT:-900}s"
  elif [[ $code != 0 ]]; then status=error note=$(grep -v -E '^ +[0-9]+ |real|maximum|resident' "$here/db/out/$out.err" | head -3 | tr '\t\n' '  ' | cut -c1-200); fi
  local wall rss
  wall=$(awk '/ real /{print $1}' "$here/db/out/$out.err")
  rss=$(awk '/maximum resident set size/{printf "%.0f", $1/1048576}' "$here/db/out/$out.err")
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$arm" "$task" "$corpus" "$tier" "$threads" "$wall" "$rss" \
    "$setup" "$lines" "$status" "$note" "$out" >> "$here/db/runs.tsv"
  echo "$arm $task $corpus $tier threads=$threads wall=$wall rss=$rss $status $note" >&2
}
section() { # FILE NAME -> the query text under '-- @NAME' up to the next marker
  awk -v want="-- @$2" '/^-- @/{on = ($0 == want); next} on' "$1"
}
