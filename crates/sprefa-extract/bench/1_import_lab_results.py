"""Import the corpus labs' results.db files into bench.db.

usage: 1_import_lab_results.py BENCH_DB LAB_NAME RESULTS_DB [LAB_NAME RESULTS_DB ...]
"""

import importlib.util
import sqlite3
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("bench_db", Path(__file__).with_name("0_bench_db.py"))
bench_db = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench_db)

TOOLS = {
    ("ryii-direct", "fast"): "ryi_fast",
    ("ryii-direct", "slow"): "ryi_slow",
    ("ryii", "fast"): "ryi_fast",
    ("ryii", "slow"): "ryi_slow",
    ("typescript-5.9.3", "oracle"): "ts_oracle",
    ("rust-analyzer", "oracle"): "rust_analyzer",
}
STATUSES = {"oracle_unavailable": "unavailable"}


def main():
    db = bench_db.open_db(sys.argv[1])
    pairs = sys.argv[2:]
    for lab, results in zip(pairs[::2], pairs[1::2]):
        source = sqlite3.connect(results)
        source.row_factory = sqlite3.Row
        rows = source.execute("SELECT * FROM corpus_score").fetchall()
        for row in rows:
            tool = TOOLS.get((row["tool"], row["mode"]), row["tool"])
            bench_db.insert(db, "row", {
                "lab": lab,
                "repo": row["repo"],
                "operation": row["operation"],
                "tool": tool,
                "target": row["target"],
                "bin_sha": "",
                "status": STATUSES.get(row["status"], row["status"]),
                "cause": None,
                "files_touched": row["files_touched"] or 0,
                "allowed_files": row["allowed_files"] or 0,
                "seconds": row["seconds"] or 0.0,
                "timeout_seconds": float(row["target_timeout_seconds"] or 0),
                "after_errors": 0,
            })
        db.commit()
        print(f"{lab}: {len(rows)} rows")


if __name__ == "__main__":
    main()
