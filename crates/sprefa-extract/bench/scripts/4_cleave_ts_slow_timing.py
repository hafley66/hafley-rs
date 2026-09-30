#!/usr/bin/env python3
"""Time TS cleave --slow with a cold CLI and a warmed daemon session."""

import csv
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[4]
BENCH = Path.home() / ".cache/lanes/claude-375/eval/bench"
LAB = Path.home() / ".cache/lanes/claude-375/recon/lab-tsserver-move-oracle"
BIN = Path.home() / ".cache/lanes/claude-375/target-writer/debug"
WORK = BENCH / "cleave-ts-slow-timing"
OUTPUT = ROOT / "plans/2026-09-28-cleave-ts-slow-timing.tsv"
ENV = {**os.environ, "XDG_CACHE_HOME": str(WORK / "cache"), "RUST_LOG": "error"}


def stop_daemon() -> None:
    socket_path = WORK / "cache/ryi/ryi.sock"
    if not socket_path.exists():
        return
    with socket.socket(socket.AF_UNIX) as stream:
        stream.connect(str(socket_path))
        stream.sendall(b"POST /__shutdown HTTP/1.1\r\nHost: ryi\r\nContent-Length: 0\r\n\r\n")
        stream.recv(1024)
    for _ in range(100):
        if not socket_path.exists():
            return
        time.sleep(0.05)
    raise RuntimeError("isolated daemon did not stop")


def invoke(binary: str, args: list[str], cwd: Path) -> tuple[int, float]:
    started = time.perf_counter()
    result = subprocess.run(
        [str(BIN / binary), *args], cwd=cwd, env=ENV,
        text=True, capture_output=True, timeout=180,
    )
    elapsed = (time.perf_counter() - started) * 1000
    return result.returncode, elapsed


def main() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / "cache").mkdir(exist_ok=True)
    targets = json.loads((LAB / "scratch/plan/targets.json").read_text())
    if len(sys.argv) > 1:
        targets = [target for target in targets if target["id"] in sys.argv[1:]]
    rows = []
    stop_daemon()
    for target in targets:
        item = target["id"]
        repo = WORK / item
        if repo.exists():
            shutil.rmtree(repo)
        subprocess.run(["git", "clone", "--quiet", target["root"], str(repo)], check=True)
        (repo / "node_modules").symlink_to(Path(target["root"]) / "node_modules", target_is_directory=True)
        dest = target["dest"]
        oracle = LAB / "oracle" / f"{item}.json"
        if target["dest_new"] and oracle.exists():
            dest = json.loads(oracle.read_text())["dest"]
        state = WORK / "state" / item
        state.mkdir(parents=True, exist_ok=True)
        args = ["cleave", f"{target['source']}#{target['symbol']}", dest,
                "--root", str(repo), "--state", str(state), "--slow"]
        warmup = invoke("ryi", args, repo)
        cold = invoke("ryii", args, repo)
        warm = invoke("ryi", args, repo)
        for mode, result in (("cold", cold), ("warm_daemon", warm)):
            rows.append({"target": item, "repo": target["repo"], "mode": mode,
                         "wall_ms": f"{result[1]:.3f}", "exit": result[0]})
        print(f"{item}: warmup={warmup[0]} cold={cold[0]} warm={warm[0]}", flush=True)
    stop_daemon()
    with OUTPUT.open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=["target", "repo", "mode", "wall_ms", "exit"], delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    print(OUTPUT)


if __name__ == "__main__":
    main()
