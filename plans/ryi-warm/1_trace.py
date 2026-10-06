import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import sys

root = Path.cwd()
out = root / "plans/ryi-warm"
label = sys.argv[1] if len(sys.argv) > 1 else "release-before"
home = out / (label + "-trace-runtime")
cache = home / "ryi"
socket = cache / "ryi.sock"
if len(os.fsencode(socket)) > 100:
    name = "ryi-" + hashlib.sha256(os.fsencode(cache)).hexdigest()[:20]
    socket = Path(os.environ.get("TMPDIR", "/tmp")) / name / "ryi.sock"
    if len(os.fsencode(socket)) > 100:
        socket = Path("/tmp") / name / "ryi.sock"
env = dict(os.environ, XDG_CACHE_HOME=str(home), RUST_LOG="off", DL_TRAIL="0",
           HAFLEY_TRACE=str(out / (label + ".trace.json")),
           HAFLEY_TRACE_FILTER="hafley_scm::read::trace=debug,hafley_scm::read::cache=debug,ryii::server_auto=info")
subprocess.run([str(Path(os.environ["CARGO_TARGET_DIR"]) / "release/ryii"), "--daemon"], env=env, check=True)
for _ in range(100):
    if socket.exists():
        break
    time.sleep(0.05)
request = json.dumps(dict(request_root=str(root), args=dict(paths=["crates/hafley_scm"], callers="0d_flow.rs#flow_edges", timeout=120)))
for state in ["cold", "warm"]:
    started = time.monotonic()
    p = subprocess.run(["curl", "--fail", "-sS", "--unix-socket", str(socket), "-H", "Content-Type: application/json", "--data", request, "http://ryi/graph"], capture_output=True, check=True)
    print(state, time.monotonic()-started, flush=True)
subprocess.run(["curl", "-sS", "--unix-socket", str(socket), "-X", "POST", "http://ryi/__shutdown"], capture_output=True)
