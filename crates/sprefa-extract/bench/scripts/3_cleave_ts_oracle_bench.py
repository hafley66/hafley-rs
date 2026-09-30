#!/usr/bin/env python3
"""Replay the classic tsserver move oracle and the current ryii cleave binary."""

import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys


HOME = Path.home()
LAB = HOME / ".cache/lanes/claude-375/recon/lab-tsserver-move-oracle"
WORK = HOME / ".cache/lanes/claude-375/eval/bench/cleave-ts-oracle"
BIN = HOME / ".cache/lanes/claude-375/target-writer/debug/ryii"
DB = HOME / ".cache/lanes/claude-375/eval/bench/bench.db"
TSV = Path(__file__).resolve().parents[4] / "plans/2026-09-28-cleave-ts-oracle-progress.tsv"

CLASSES = {
    "declaration": "vite-02 vite-03 vite-09 cg-02 cg-03 cg-05 cg-08 cg-10",
    "destination_export": "vite-01 vite-04 vite-10 cg-09",
    "import_kind": "vite-06 cg-04 cg-06",
    "moved_dependencies": "vite-01 vite-08",
    "corrupt_edit": "cg-07",
    "unused_import": "cg-01 vite-10",
}


def prepare() -> None:
    if WORK.exists():
        shutil.rmtree(WORK)
    (WORK / "scratch").mkdir(parents=True)
    shutil.copytree(LAB / "scratch/plan", WORK / "scratch/plan")
    shutil.copy2(LAB / "scratch/run.py", WORK / "scratch/run.py")
    shutil.copy2(LAB / "scratch/ts-oracle.cjs", WORK / "scratch/ts-oracle.cjs")
    (WORK / "scratch/ts-oracle").symlink_to(LAB / "scratch/ts-oracle", target_is_directory=True)
    (WORK / "oracle").symlink_to(LAB / "oracle", target_is_directory=True)
    runner = WORK / "scratch/run.py"
    source = runner.read_text().replace("['ryii', 'cleave'", "[os.environ['RYII_BIN'], 'cleave'")
    if source == runner.read_text():
        raise RuntimeError("oracle runner no longer has the expected ryii command")
    source = source.replace("('tsserver', 'ryi')", "('tsserver', 'ryi', 'ryi_slow')")
    source = source.replace("state = HERE / 'scratch/state' / t['id']", "state = HERE / 'scratch/state' / t['id'] / tool")
    source = source.replace("'--commit', '--json'], root)", "'--commit', '--json', *(['--slow'] if tool == 'ryi_slow' else [])], root)")
    source = source.replace("/ 'ryi.log'", "/ f'{tool}.log'")
    runner.write_text(source)


def publish() -> None:
    after = json.loads((WORK / "scratch/results.json").read_text())
    before = json.loads((LAB / "scratch/results.json").read_text())
    DB.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(DB) as destination, sqlite3.connect(WORK / "bench.db") as source:
        destination.execute("DROP TABLE IF EXISTS cleave_check")
        schema = source.execute("SELECT sql FROM sqlite_master WHERE name='cleave_check'").fetchone()[0]
        destination.execute(schema)
        rows = source.execute("SELECT * FROM cleave_check").fetchall()
        destination.executemany("INSERT INTO cleave_check VALUES (?,?,?,?,?,?,?,?,?)", rows)
    lines = ["defect\ttarget\ttsserver_pass\tryi_before_pass\tryi_after_pass\tryi_slow_pass"]
    for defect, ids in CLASSES.items():
        for target in ids.split():
            old = before[target]["outcomes"]
            new = after[target]["outcomes"]
            passed = lambda row: int(row["tool_exit"] == 0 and row["typecheck_equal"])
            lines.append(
                f"{defect}\t{target}\t{passed(new['tsserver'])}\t"
                f"{passed(old['ryi'])}\t{passed(new['ryi'])}\t{passed(new['ryi_slow'])}"
            )
    TSV.write_text("\n".join(lines) + "\n")
    print(f"{len(rows)} cleave_check rows in {DB}")
    print(f"{len(lines) - 1} defect rows in {TSV}")
    missed = [
        target
        for target, result in after.items()
        if result["outcomes"]["tsserver"]["tool_exit"] == 0
        and result["outcomes"]["tsserver"]["typecheck_equal"]
        and (
            result["outcomes"]["ryi"]["tool_exit"] != 0
            or not result["outcomes"]["ryi"]["typecheck_equal"]
        )
    ]
    if missed:
        raise AssertionError(f"ryi missed oracle-passing targets: {missed}")
    slow_passes = sum(
        result["outcomes"]["ryi_slow"]["tool_exit"] == 0
        and result["outcomes"]["ryi_slow"]["typecheck_equal"]
        for result in after.values()
    )
    oracle_passes = sum(
        result["outcomes"]["tsserver"]["tool_exit"] == 0
        and result["outcomes"]["tsserver"]["typecheck_equal"]
        for result in after.values()
    )
    if slow_passes < oracle_passes:
        raise AssertionError(f"ryi_slow {slow_passes}/20 < tsserver {oracle_passes}/20")


def main() -> None:
    if not BIN.is_file():
        raise FileNotFoundError(BIN)
    prepare()
    env = dict(os.environ, RYII_BIN=str(BIN), RUST_LOG="error")
    subprocess.run([sys.executable, str(WORK / "scratch/run.py")], cwd=WORK, env=env, check=True)
    publish()


if __name__ == "__main__":
    main()
