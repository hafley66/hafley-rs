#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
export CARGO_BUILD_JOBS=2
case "${1:-check}" in
  encode)
    lab_encode "$2" "$3"
    lab_probe "$3"
    ;;
  build)
    node "$lab_dir/97a_sources.mjs"
    node "$lab_dir/contracts/1_generate.mjs" --check
    lab_cargo run --bin falcon-web-bake -- "$lab_dir/94_web/1_assets.bin"
    cd "$lab_dir/94_web"
    node 3_deploy.mjs export
    node 3_deploy.mjs check-export
    ;;
  check|plan)
    cd "$lab_dir/94_web"
    if [ "${1:-check}" = plan ]; then node 3_deploy.mjs plan; else node 3_deploy.mjs check-export; fi
    ;;
  *) printf 'Unknown web action: %s\n' "$1" >&2; exit 2 ;;
esac
