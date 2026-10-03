"""Q1: every scm++ case of the stress-lab oracle, compiled once, run as SQL text on SQLite and DuckDB over one store.

usage: python3 1_query_breadth.py small|crates [CASE_PREFIX]  -> db/raw.tsv rows (run Q1), db/q1-<corpus>.tsv (rows equal)
"""
import hashlib, importlib.util, os, pathlib, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(HERE))
measure = importlib.import_module("0_measure").measure
spec = importlib.util.spec_from_file_location("oracle", HERE.parent / "lab-20261002-ryi-scmpp-stress" / "2_oracle.py")
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

RYII = os.environ.get("RYII", str(REPO / "crates/sprefa-extract/target/release/ryii"))
SQL_BIN = os.environ.get("SQL_BIN", str(HERE / "4_writer/target/sql/release/lab_writer"))
SQLITE = os.environ.get("SQLITE", "/opt/homebrew/opt/sqlite/bin/sqlite3")
DUCKDB = os.environ.get("DUCKDB", "/opt/homebrew/bin/duckdb")
INPUTS = {"small": ["crates/hafley_scm/src", "crates/sprefa-extract/src"], "crates": ["crates"]}
TABLES = ["scmpp_dict_path", "scmpp_dict_kind", "scmpp_dict_field", "scmpp_dict_capture", "scmpp_dict_text", "scmpp_node", "scmpp_capture"]
# unit and record separators: no CLI quoting, newlines inside captured text stay inside one row
SQLITE_DUMP = ["-batch", "-list", "-separator", "\x1f", "-newline", "\x1e", "-nullvalue", "NULL"]
DUCKDB_DUMP = ["-batch", "-noheader", "-list", "-separator", "\x1f", "-newline", "\x1e", "-nullvalue", "NULL"]


def rows_of(path):
    return [row for row in path.read_bytes().split(b"\x1e") if row]


def main(corpus, prefix=""):
    work = HERE / "db" / "q1"
    (work / "sql").mkdir(parents=True, exist_ok=True)
    out = HERE / "db" / "out" / "Q1" / corpus
    out.mkdir(parents=True, exist_ok=True)
    store, duck = work / f"store-{corpus}.db", work / f"store-{corpus}.duckdb"
    summary = HERE / "db" / f"q1-{corpus}.tsv"
    if not summary.exists():
        summary.write_text("case\tcorpus\tryii_rows\tsqlite_rows\tduckdb_rows\trows_equal\torder_equal\tsqlite_digest\n")
    for case, here, relation, there, option in oracle.CASES:
        if not case.startswith(prefix):
            continue
        name = case.replace("/", "__")
        scm, sql_path = work / "sql" / f"{name}.scm", work / "sql" / f"{name}.sql"
        scm.write_text(oracle.query(here, relation, there, option) + "\n")
        if not sql_path.exists():
            sql_path.write_text(subprocess.run([SQL_BIN, "sql", str(scm)], check=True, capture_output=True, text=True).stdout)
        sql = sql_path.read_text().strip()
        store.unlink(missing_ok=True)
        built = measure("Q1", case, corpus, "ryii", "store", out / f"{name}.ryii.txt",
                        [RYII, "query", "--scmpp", str(scm), "--sqlite", str(store), "--pattern", "*.rs",
                         *[str(REPO / path) for path in INPUTS[corpus]]], repeat=1)
        if built[-1][9] != "ok":
            continue
        duck.unlink(missing_ok=True)
        load = "LOAD sqlite; " + " ".join(f"CREATE TABLE {t} AS SELECT * FROM sqlite_scan('{store}', '{t}');" for t in TABLES)
        measure("Q1", case, corpus, "duckdb", "load", out / f"{name}.load.txt", [DUCKDB, str(duck), "-c", load], repeat=1)
        measure("Q1", case, corpus, "sqlite", "query", out / f"{name}.sqlite.time.txt",
                [SQLITE, "-readonly", str(store), f"PRAGMA temp_store=MEMORY; CREATE TEMP TABLE bench_row AS {sql};"])
        measure("Q1", case, corpus, "duckdb", "query", out / f"{name}.duckdb.time.txt",
                [DUCKDB, "-readonly", str(duck), "-c", f"CREATE TEMP TABLE bench_row AS {sql};"])
        lite_out, duck_out = out / f"{name}.sqlite.rows", out / f"{name}.duckdb.rows"
        with lite_out.open("wb") as handle:
            subprocess.run([SQLITE, *SQLITE_DUMP, "-readonly", str(store), sql + ";"], stdout=handle, check=True)
        with duck_out.open("wb") as handle:
            subprocess.run([DUCKDB, *DUCKDB_DUMP, "-readonly", str(duck), "-c", sql + ";"], stdout=handle, check=True)
        lite, duckrows = rows_of(lite_out), rows_of(duck_out)
        ryii_rows = subprocess.run([SQLITE, str(store), "SELECT count(*) FROM scmpp_row"], capture_output=True, text=True).stdout.strip()
        line = [case, corpus, ryii_rows, str(len(lite)), str(len(duckrows)),
                "yes" if sorted(lite) == sorted(duckrows) else "no", "yes" if lite == duckrows else "no",
                hashlib.sha256(b"\x1e".join(sorted(lite))).hexdigest()[:16]]
        with summary.open("a") as handle:
            handle.write("\t".join(line) + "\n")
        print(*line, sep="\t", flush=True)
        lite_out.unlink(); duck_out.unlink()
    store.unlink(missing_ok=True)
    duck.unlink(missing_ok=True)


if __name__ == "__main__":
    main(*sys.argv[1:])
