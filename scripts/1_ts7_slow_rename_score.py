"""Add measured slow_rename edit sites to bench/score.py's score table."""

import json
import sqlite3
from pathlib import Path


BENCH = Path.home() / ".cache/lanes/claude-375/eval/bench"
truth = sqlite3.connect(BENCH / "truth.db")
bench = sqlite3.connect(BENCH / "bench.db")
rows = json.loads((BENCH / "slow-rename-work/results.json").read_text())
bench.execute("delete from score where tool='ryi' and endpoint='slow_rename'")
for row in rows:
    repo, symbol = row["repo"], row["symbol"]
    target = truth.execute("select name,kind,bucket from target where repo=? and symbol=?", (repo, symbol)).fetchone()
    expected = set(truth.execute("select path,line from occ where repo=? and symbol=? and is_def=0", (repo, symbol)))
    found = {tuple(site) for site in row["changed_sites"]}
    truth_files = {path for path, _ in expected}
    found_files = {path for path, _ in found}
    file_tp = len(found_files & truth_files)
    site_tp = len(found & expected)
    bench.execute("insert into score values (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
        repo, "ryi", "slow_rename", symbol, *target, row["return_code"], row["wall_seconds"],
        len(truth_files), len(found_files), file_tp,
        file_tp / len(found_files) if found_files else None,
        file_tp / len(truth_files) if truth_files else None,
        len(expected), len(found), site_tp,
        site_tp / len(found) if found else None,
        site_tp / len(expected) if expected else None, None,
    ))
bench.commit()
print(len(rows), "slow_rename scored rows")
