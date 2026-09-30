"""bench.db: tables from schema/generated/8_bench.sql, values checked against 8_bench.json."""

import json
import sqlite3
from pathlib import Path

GENERATED = Path(__file__).resolve().parents[1] / "schema/generated"
CATALOG = json.loads((GENERATED / "8_bench.json").read_text())


def open_db(path):
    db = sqlite3.connect(path)
    db.executescript((GENERATED / "8_bench.sql").read_text())
    return db


def insert(db, table, row):
    columns = CATALOG["columns"][table]
    names = [column["name"] for column in columns]
    unknown = set(row) - set(names)
    if unknown:
        raise ValueError(f"{table}: no column {sorted(unknown)}")
    for column in columns:
        allowed = CATALOG["enums"].get(column.get("enum", ""))
        if allowed is not None and row.get(column["name"]) not in allowed:
            raise ValueError(f"{table}.{column['name']} = {row.get(column['name'])!r}; allowed {allowed}")
    db.execute(
        f"INSERT OR REPLACE INTO {table} ({', '.join(names)}) VALUES ({', '.join('?' for _ in names)})",
        [row.get(name) for name in names],
    )
