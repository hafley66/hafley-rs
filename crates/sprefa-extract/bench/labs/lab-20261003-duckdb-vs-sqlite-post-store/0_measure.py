"""One timed command, REPEAT times under /usr/bin/time -l; one row per repeat in db/raw.tsv; last stdout -> OUT.

usage: python3 0_measure.py RUN CASE CORPUS ENGINE VARIANT OUT -- command...   (env REPEAT, default 3; TIMEOUT, default 900)
"""
import os, pathlib, re, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
DB = HERE / "db"
RAW = DB / "raw.tsv"
COLUMNS = ["run", "case", "corpus", "engine", "variant", "repeat", "load_average_1m", "wall_s", "peak_rss_mb", "status", "note", "output"]


def measure(run, case, corpus, engine, variant, out, argv, repeat=None, timeout=None, stdin=None):
    repeat = int(repeat or os.environ.get("REPEAT", "3"))
    timeout = int(timeout or os.environ.get("TIMEOUT", "900"))
    DB.mkdir(exist_ok=True)
    out = pathlib.Path(out).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    if not RAW.exists():
        RAW.write_text("\t".join(COLUMNS) + "\n")
    rows = []
    for index in range(repeat):
        load = f"{os.getloadavg()[0]:.2f}"
        status, note = "ok", ""
        with out.open("wb") as handle:
            try:
                done = subprocess.run(["/usr/bin/time", "-l", *argv], stdout=handle, stderr=subprocess.PIPE,
                                      timeout=timeout, input=stdin)
                err = done.stderr.decode(errors="replace")
                if done.returncode != 0:
                    status = "error"
                    note = " ".join(line for line in err.splitlines()
                                    if line.strip() and not re.match(r"^\s+\d+\s+\S", line) and " real " not in line)[:240]
            except subprocess.TimeoutExpired:
                err, status, note = "", "error", f"timeout {timeout}s"
        wall = re.search(r"([\d.]+) real", err)
        rss = re.search(r"(\d+)\s+maximum resident set size", err)
        row = [run, case, corpus, engine, variant, str(index), load, wall.group(1) if wall else "",
               f"{int(rss.group(1)) / 1048576:.0f}" if rss else "", status, note.replace("\t", " "), str(out.relative_to(HERE))]
        with RAW.open("a") as handle:
            handle.write("\t".join(row) + "\n")
        rows.append(row)
        print(" ".join(row[:5]), "wall", row[7], "rss", row[8], status, note[:120], file=sys.stderr)
        if status != "ok":
            break
    return rows


if __name__ == "__main__":
    split = sys.argv.index("--")
    measure(*sys.argv[1:split], sys.argv[split + 1:])
