"""Compare cold ryii calls with an isolated warm ryi daemon."""

import csv
import os
import re
import shutil
import socket
import sqlite3
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
BENCH = Path.home() / ".cache/lanes/claude-375/eval/bench"
BIN = Path.home() / ".cache/lanes/claude-375/target-writer/debug"
CACHE = BENCH / "daemon-timing-cache"
STATE = BENCH / "daemon-timing-state"
OUTPUT = ROOT / "plans/2026-09-28-slow-tier-daemon-timing.tsv"
ENV = {**os.environ, "XDG_CACHE_HOME": str(CACHE), "RUST_LOG": "error"}
TIME_RSS = re.compile(rb"(\d+)\s+maximum resident set size")
STAGE = re.compile(rb"^stage [0-9a-f]+ dry run, tree untouched$", re.MULTILINE)


def run(binary: str, args: list[str], cwd: Path) -> tuple[int, bytes, float, float]:
    command = [str(BIN / binary), *args]
    if binary == "ryii":
        command = ["/usr/bin/time", "-l", *command]
    started = time.perf_counter()
    result = subprocess.run(command, cwd=cwd, env=ENV, capture_output=True, timeout=180)
    elapsed = (time.perf_counter() - started) * 1000
    if binary == "ryii":
        match = TIME_RSS.search(result.stderr)
        if not match:
            raise RuntimeError(f"missing cold RSS: {result.stderr[-500:]!r}")
        rss = int(match.group(1)) / (1024 * 1024)
    else:
        pid = int((CACHE / "ryi/ryi.pid").read_text())
        sampled = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)])
        rss = int(sampled.strip()) / 1024
    return result.returncode, STAGE.sub(b"stage <id> dry run, tree untouched", result.stdout), elapsed, rss


def stop_daemon() -> None:
    path = CACHE / "ryi/ryi.sock"
    if not path.exists():
        return
    with socket.socket(socket.AF_UNIX) as stream:
        stream.connect(str(path))
        stream.sendall(b"POST /__shutdown HTTP/1.1\r\nHost: ryi\r\nContent-Length: 0\r\n\r\n")
        stream.recv(1024)
    for _ in range(100):
        if not path.exists():
            return
        time.sleep(0.05)
    raise RuntimeError("isolated daemon did not stop")


def row(tool: str, repo: str, operation: str, mode: str, samples: list[tuple[float, float]]) -> dict:
    elapsed = sorted(sample[0] for sample in samples)
    rss = max(sample[1] for sample in samples)
    return {
        "tool": tool,
        "repo": repo,
        "operation": operation,
        "mode": mode,
        "runs": len(samples),
        "median_ms": f"{elapsed[len(elapsed) // 2]:.3f}",
        "p90_ms": f"{elapsed[(9 * len(elapsed) + 9) // 10 - 1]:.3f}",
        "peak_rss_mib": f"{rss:.3f}",
    }


def compare(tool: str, repo: str, operation: str, args: list[str], cwd: Path, runs: int,
            warm_args: list[str] | None = None) -> list[dict]:
    stop_daemon()
    warm_args = args if warm_args is None else warm_args
    warmed = run("ryi", warm_args, cwd)
    if warmed[0] != 0:
        raise RuntimeError(f"warmup failed: {tool} {repo} {operation}: {warmed[0]}")
    cold_samples = []
    warm_samples = []
    for _ in range(runs):
        cold = run("ryii", args, cwd)
        warm = run("ryi", warm_args, cwd)
        if cold[0] != 0 or warm[0] != 0 or cold[1] != warm[1]:
            raise AssertionError(f"cold/warm mismatch: {tool} {repo} {operation}: {cold[0]}, {warm[0]}")
        cold_samples.append(cold[2:])
        warm_samples.append(warm[2:])
    print(f"{repo} {operation}: {runs} equal cold/warm results", flush=True)
    return [row(tool, repo, operation, "cold", cold_samples),
            row(tool, repo, operation, "warm", warm_samples)]


def rename_targets() -> list[tuple[str, str, list[str], Path]]:
    truth = sqlite3.connect(BENCH / "truth.db")
    targets = truth.execute("""select repo,name,def_path,def_line from target
        where repo in ('vite','codegraph-src') and (def_path like '%.ts' or def_path like '%.tsx')
        order by repo,def_path,def_line,name""").fetchall()
    result = []
    for repo, name, path, line in targets:
        root = BENCH / "runs/ryii" / repo
        old = name.removesuffix(":").rsplit(":", 1)[-1]
        lines = (root / path).read_text().splitlines()
        at = sum(len(part.encode()) + 1 for part in lines[:line - 1]) + lines[line - 1].find(old)
        if old not in lines[line - 1]:
            raise RuntimeError(f"missing declaration offset: {repo} {name}")
        args = ["rename", f"{path}#{old}", f"{old}_zz", "--root", str(root),
                "--state", str(STATE), "--slow", "--at", str(at), "--json"]
        result.append((repo, name, args, root))
    return result


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    STATE.mkdir(parents=True, exist_ok=True)
    rows = []
    soopy = ROOT / "crates/soopy"
    index = ROOT / "crates/sprefa-extract/tests/fixtures/ratchet_soopy/index.scip"
    graph = ["graph", "--slow", "--uses", "SyncMeter", "--root", str(soopy),
             "--scip-index", str(index), "--timeout", "120", str(soopy / "src")]
    rows.extend(compare("ryi", "hafley-rs", "graph --slow", graph, soopy, 3))

    source = ROOT / "crates/sprefa-extract/tests/fixtures/cleave_ratchet/private_fields"
    fixture = BENCH / "warm-cleave-fixture"
    shutil.rmtree(fixture, ignore_errors=True)
    shutil.copytree(source, fixture)
    subprocess.run(["git", "init", "-q", "."], cwd=fixture, check=True)
    subprocess.run(["git", "add", "-A"], cwd=fixture, check=True)
    cleave = ["cleave", "src/source.rs#Packet", "src/dest.rs", "--root", str(fixture),
              "--state", str(STATE), "--json", "--slow"]
    rows.extend(compare("ryi", "hafley-rs", "cleave private-field widen", cleave, fixture, 3))

    targets = rename_targets()
    for repo in ("codegraph-src", "vite"):
        selected = [(name, args, root) for target_repo, name, args, root in targets if target_repo == repo]
        stop_daemon()
        first = selected[0]
        if run("ryi", first[1], first[2])[0] != 0:
            raise RuntimeError(f"warmup failed: {repo}")
        cold_samples = []
        warm_samples = []
        for name, args, root in selected:
            cold = run("ryii", args, root)
            warm = run("ryi", args, root)
            if cold[0] != 0 or warm[0] != 0 or cold[1] != warm[1]:
                raise AssertionError(f"cold/warm rename mismatch: {repo} {name}: {cold[0]}, {warm[0]}")
            cold_samples.append(cold[2:])
            warm_samples.append(warm[2:])
        print(f"{repo} slow rename: {len(selected)} equal cold/warm results", flush=True)
        rows.extend([row("ryi", repo, "slow rename", "cold", cold_samples),
                     row("ryi", repo, "slow rename", "warm", warm_samples)])
    stop_daemon()
    with OUTPUT.open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=["tool", "repo", "operation", "mode", "runs",
                                                 "median_ms", "p90_ms", "peak_rss_mib"],
                                delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    print(OUTPUT)


if __name__ == "__main__":
    main()
