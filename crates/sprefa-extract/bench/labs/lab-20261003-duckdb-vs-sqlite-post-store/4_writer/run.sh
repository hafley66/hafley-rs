#!/usr/bin/env bash
# usage: 4_writer/run.sh b1 | sql | w1 [small|crates ...]
# b1: clean + incremental release builds with duckdb (target/duck) and without (target/plain) -> db/b1.tsv
# sql: the Q1 SQL printer build (target/sql). w1: lab_writer write per engine, 3 processes each -> raw.tsv run W1.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
lab=$(dirname "$here")
export RCARGO_OFF=1 KACHE_DISABLED=1  # local cargo, no rustc cache: clean means clean
build() { # name cargo-args... ; prints wall seconds
  local start=$(python3 -c 'import time; print(time.time())')
  (cd "$here" && CARGO_TARGET_DIR=target/$1 cargo build --release "${@:2}" >"$lab/db/b1-$1.log" 2>&1)
  python3 -c "import time; print(f'{time.time() - $start:.1f}')"
}
case $1 in
  b1)
    out=$lab/db/b1.tsv
    printf 'variant\tstep\twall_s\tload_average_1m\tbinary_bytes\n' > "$out"
    for variant in duck plain; do
      args=(); [[ $variant == plain ]] && args=(--no-default-features)
      load=$(sysctl -n vm.loadavg | awk '{print $2}')
      if [[ $variant == duck && -n ${DUCK_CLEAN:-} ]]; then # an already-timed clean build: "wall_s load_average_1m"
        read -r wall load <<<"$DUCK_CLEAN"
      else
        rm -rf "$here/target/$variant"; wall=$(build "$variant" "${args[@]}")
      fi
      printf '%s\tclean\t%s\t%s\t%s\n' "$variant" "$wall" "$load" "$(/usr/bin/stat -f %z "$here/target/$variant/release/lab_writer")" >> "$out"
      for i in 1 2 3; do
        touch "$here/src/main.rs"
        load=$(sysctl -n vm.loadavg | awk '{print $2}'); wall=$(build "$variant" "${args[@]}")
        printf '%s\tincremental\t%s\t%s\t%s\n' "$variant" "$wall" "$load" "$(/usr/bin/stat -f %z "$here/target/$variant/release/lab_writer")" >> "$out"
      done
    done ;;
  sql) (cd "$here" && CARGO_TARGET_DIR=target/sql cargo build --release --no-default-features --features sql) ;;
  w1)
    shift
    for corpus in ${@:-small crates}; do
      for engine in none sqlite duckdb duckdb_pk; do
        for i in 1 2 3; do
          REPEAT=1 python3 "$lab/0_measure.py" W1 write "$corpus" "$engine" "r$i" "$lab/db/out/W1/$corpus.$engine.r$i.tsv" -- \
            "$here/target/duck/release/lab_writer" write "$engine" "$lab/db/ancestor-$corpus.db" "$lab/db/w1-$corpus.$engine"
        done
      done
    done ;;
esac
