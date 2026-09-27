#!/usr/bin/env bash
set -euo pipefail

fixture_root=$(cd "$(dirname "$0")" && pwd -P)
cache_root=${XDG_CACHE_HOME:-$HOME/.cache}
lock_path="$cache_root/ryi-vs-codeql.lock"
mkdir -p "$cache_root"

# Share the one-machine CodeQL/JVM queue. The lock remains held while
# scip-java and the Gradle child process run.
exec python3 - "$lock_path" "$fixture_root" <<'PY'
import fcntl
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

lock_path = pathlib.Path(sys.argv[1])
fixture_root = pathlib.Path(sys.argv[2])
with lock_path.open("a") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    scip_java = shutil.which("scip-java")
    gradle = shutil.which("gradle")
    if not scip_java:
        raise SystemExit("regen: scip-java must be on PATH")
    if not gradle:
        raise SystemExit("regen: gradle must be on PATH")

    fd, output_name = tempfile.mkstemp(prefix=".index.", suffix=".scip", dir=fixture_root)
    os.close(fd)
    output = pathlib.Path(output_name)
    output.unlink()
    try:
        subprocess.run(
            [scip_java, "index", "--output", str(output)],
            cwd=fixture_root,
            check=True,
        )
        if not output.is_file() or output.stat().st_size == 0:
            raise SystemExit("regen: scip-java did not produce a nonempty index")
        os.replace(output, fixture_root / "index.scip")
    finally:
        output.unlink(missing_ok=True)
        subprocess.run([gradle, "--stop"], cwd=fixture_root, check=False)
PY
