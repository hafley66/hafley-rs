"""X1: one appending writer process, two reader processes querying until it exits.

usage: python3 7_concurrency.py [ENGINE ...]   ENGINE = sqlite | duckdb | duckdb_reopen  -> db/x1.tsv (one row per reader query),
db/x1-writer.tsv (one row per writer batch). A read is consistent when every batch it sees is whole: count = batches * ROWS
and batches = max(batch) + 1. Each reader query is a fresh CLI process (the sprefa reader shape).
"""
import os, pathlib, subprocess, sys, threading, time

HERE = pathlib.Path(__file__).resolve().parent
WRITER = os.environ.get("WRITER", str(HERE / "4_writer/target/duck/release/lab_writer"))
SQLITE = os.environ.get("SQLITE", "/opt/homebrew/opt/sqlite/bin/sqlite3")
DUCKDB = os.environ.get("DUCKDB", "/opt/homebrew/bin/duckdb")
BATCHES, ROWS, PAUSE_MS = int(os.environ.get("BATCHES", "200")), int(os.environ.get("ROWS", "1000")), int(os.environ.get("PAUSE_MS", "20"))
QUERY = "SELECT count(*), count(DISTINCT batch), coalesce(max(batch), -1) FROM x1_row;"


def reader(engine, path, reader_id, done, rows):
    argv = ([SQLITE, "-readonly", "-batch", "-list", "-separator", "\t", str(path), QUERY] if engine == "sqlite"
            else [DUCKDB, "-readonly", "-batch", "-noheader", "-list", "-separator", "\t", str(path), "-c", QUERY])
    query = 0
    while not done.is_set():
        started = time.time()
        result = subprocess.run(argv, capture_output=True, text=True)
        latency = time.time() - started
        count = batches = top = ""
        status, consistent, error = "ok", "", ""
        if result.returncode == 0 and result.stdout.strip():
            count, batches, top = result.stdout.strip().split("\t")
            consistent = "yes" if int(count) == int(batches) * ROWS and int(batches) == int(top) + 1 else "no"
        else:
            status = "error"
            error = (result.stderr.strip().splitlines() or [""])[0][:200].replace("\t", " ")
        rows.append([engine, str(reader_id), str(query), f"{started:.3f}", f"{latency:.4f}", status, count, batches, consistent, error])
        query += 1


def run(engine):
    path = HERE / "db" / f"x1.{engine}"
    for suffix in ("", "-wal", "-shm", ".wal"):
        pathlib.Path(str(path) + suffix).unlink(missing_ok=True)
    if engine == "sqlite":
        subprocess.run([SQLITE, str(path), "PRAGMA journal_mode=WAL; CREATE TABLE x1_row (batch INTEGER NOT NULL, seq INTEGER NOT NULL, value INTEGER NOT NULL);"],
                       check=True, capture_output=True)
    else:
        subprocess.run([DUCKDB, str(path), "-c", "CREATE TABLE x1_row (batch BIGINT NOT NULL, seq BIGINT NOT NULL, value BIGINT NOT NULL);"],
                       check=True, capture_output=True)
    load = os.getloadavg()[0]
    done, rows = threading.Event(), []
    started = time.time()
    writer = subprocess.Popen([WRITER, "append", engine, str(path), str(BATCHES), str(ROWS), str(PAUSE_MS)],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    threads = [threading.Thread(target=reader, args=(engine, path, index, done, rows)) for index in (1, 2)]
    for thread in threads:
        thread.start()
    out, err = writer.communicate()
    writer_wall = time.time() - started
    done.set()
    for thread in threads:
        thread.join()
    final = subprocess.run([SQLITE, "-separator", "\t", str(path), QUERY] if engine == "sqlite"
                           else [DUCKDB, "-readonly", "-noheader", "-list", "-separator", "\t", str(path), "-c", QUERY],
                           capture_output=True, text=True).stdout.strip()
    with (HERE / "db" / "x1.tsv").open("a") as handle:
        for row in rows:
            handle.write("\t".join(row) + "\n")
    with (HERE / "db" / "x1-writer.tsv").open("a") as handle:
        for line in out.splitlines():
            handle.write(f"{engine}\t{line}\n")
        if writer.returncode != 0:
            handle.write(f"{engine}\twriter_exit\t{writer.returncode}\t0\t{err.strip()[:200]}\n")
    with (HERE / "db" / "x1-run.tsv").open("a") as handle:
        handle.write(f"{engine}\t{load:.2f}\t{writer_wall:.2f}\t{writer.returncode}\t{final}\n")
    print(engine, "writer", f"{writer_wall:.2f}s", "exit", writer.returncode, "final", final, "reader queries", len(rows),
          "errors", sum(r[5] == "error" for r in rows), "inconsistent", sum(r[8] == "no" for r in rows), err.strip()[:200])


if __name__ == "__main__":
    for name, header in (("x1.tsv", "engine\treader\tquery\tstarted_unix\tlatency_s\tstatus\tcount\tbatches\tconsistent\terror"),
                         ("x1-writer.tsv", "engine\tbatch\tcommit_unix_ms\topen_retries\terror"),
                         ("x1-run.tsv", "engine\tload_average_1m\twriter_wall_s\twriter_exit\tfinal_count\tfinal_batches\tfinal_max_batch")):
        target = HERE / "db" / name
        if not target.exists():
            target.write_text(header + "\n")
    for engine in sys.argv[1:] or ["sqlite", "duckdb", "duckdb_reopen"]:
        run(engine)
