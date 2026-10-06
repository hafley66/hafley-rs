import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path.cwd()
out = root / "plans/ryi-warm"
label = sys.argv[1]
binary = Path(os.environ["CARGO_TARGET_DIR"]) / "release/ryii"
home = out / (label + "-runtime")
cache = home / "ryi"
socket = cache / "ryi.sock"
if len(os.fsencode(socket)) > 100:
    name = "ryi-" + hashlib.sha256(os.fsencode(cache)).hexdigest()[:20]
    socket = Path(os.environ.get("TMPDIR", "/tmp")) / name / "ryi.sock"
    if len(os.fsencode(socket)) > 100:
        socket = Path("/tmp") / name / "ryi.sock"
env = dict(os.environ, XDG_CACHE_HOME=str(home), RUST_LOG="off", DL_TRAIL="0")
env.pop("HAFLEY_TRACE", None)
rows = []

def run(path, state, args, command):
    started = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True)
    seconds = time.monotonic() - started
    (out / f"{label}-{path}-{state}.out").write_bytes(result.stdout)
    (out / f"{label}-{path}-{state}.err").write_bytes(result.stderr)
    row = dict(path=path, state=state, seconds=seconds, status=result.returncode)
    rows.append(row)
    print(json.dumps(row), flush=True)
    if result.returncode:
        raise RuntimeError(result.stderr.decode())

def request(written=False):
    args = dict(paths=["crates/hafley_scm"], callers="0d_flow.rs#flow_edges", timeout=120)
    if written:
        args["fast_receivers"] = "written"
    return ["curl", "--fail", "-sS", "--unix-socket", str(socket),
            "-H", "Content-Type: application/json", "--data",
            json.dumps(dict(request_root=str(root), args=args)), "http://ryi/graph"]

cli = [str(binary), "graph", "--callers", "0d_flow.rs#flow_edges", "crates/hafley_scm"]
for state in ["cold", "warm"]:
    run("one-shot", state, None, cli)
subprocess.run([str(binary), "--daemon"], env=env, check=True)
for _ in range(100):
    if socket.exists():
        break
    time.sleep(0.05)
try:
    for written, path in [(False, "fast"), (True, "written")]:
        for state in ["cold", "warm"]:
            run(path, state, None, request(written))
finally:
    subprocess.run(["curl", "-sS", "--unix-socket", str(socket), "-X", "POST",
                    "http://ryi/__shutdown"], capture_output=True)
(out / f"{label}.json").write_text(json.dumps(rows, indent=2))
