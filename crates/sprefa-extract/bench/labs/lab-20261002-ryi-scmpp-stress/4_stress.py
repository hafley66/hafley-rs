"""scm++ wall time, peak RSS, DB size and row counts per (binary, corpus, query); appends stress.tsv.

usage: python3 4_stress.py BINARY LABEL [QUERY_NAME...]
"""
import os, re, sqlite3, subprocess, sys, time, pathlib

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent.parent
WT = BENCH.parent.parent.parent
DBDIR = BENCH / "repos" / "stress-db"
CORPORA = {
    "hafley-rs-crates": (WT / "crates", ["--pattern", "*.rs"], "rust"),
    "hafley-rxjs-packages": (BENCH / "repos" / "hafley-rxjs-main" / "packages", ["--pattern", "*.ts", "--pattern", "*.tsx"], "ts"),
}
QUERIES = {
    "plain_calls": {"rust": "((call_expression) @call)", "ts": "((call_expression) @call)"},
    "ancestor_fn": {"rust": "((call_expression) @call (#has-ancestor? @call function_item))",
                    "ts": "((call_expression) @call (#has-ancestor? @call function_declaration))"},
    "ancestor_each": {"rust": "((call_expression) @call (#has-ancestor? @call (function_item name: (identifier) @fn) rows: each))",
                      "ts": "((call_expression) @call (#has-ancestor? @call (function_declaration name: (identifier) @fn) rows: each))"},
    "has_return": {"rust": "((function_item body: (block) @body) @f (#has? @body return_expression))",
                   "ts": "((function_declaration body: (statement_block) @body) @f (#has? @body return_statement))"},
    "parent_stmt": {"rust": "((call_expression) @call (#has-parent? @call expression_statement))",
                    "ts": "((call_expression) @call (#has-parent? @call expression_statement))"},
    "precedes_neighbor": {"rust": "((let_declaration) @let (#precedes? @let expression_statement stopBy: neighbor))",
                          "ts": "((lexical_declaration) @let (#precedes? @let expression_statement stopBy: neighbor))"},
    "nth_of": {"rust": "((let_declaration) @let (#nth-child? @let 2 of let_declaration))",
               "ts": "((lexical_declaration) @let (#nth-child? @let 2 of lexical_declaration))"},
    "nested3_each": {"rust": "((identifier) @id (#has-ancestor? @id ((closure_expression) @clo (#has-ancestor? @clo ((function_item name: (identifier) @fn) @f (#not-has-ancestor? @f impl_item)) rows: each)) rows: each))",
                     "ts": "((identifier) @id (#has-ancestor? @id ((arrow_function) @clo (#has-ancestor? @clo ((function_declaration name: (identifier) @fn) @f (#not-has-ancestor? @f class_declaration)) rows: each)) rows: each))"},
}


def main(binary, label, names):
    out = HERE / "stress.tsv"
    if not out.exists():
        out.write_text("binary\tcorpus\tquery\texit_code\twall_seconds\tpeak_rss_bytes\tdb_bytes\tcapture_rows\tedge_rows\tnode_rows\tresult_rows\terror\n")
    for corpus, (path, extra, lang) in CORPORA.items():
        for name in names or QUERIES:
            scm = DBDIR / "stress.scm"
            scm.write_text(QUERIES[name][lang] + "\n")
            db = DBDIR / f"stress-{label}-{corpus}-{name}.db"
            db.unlink(missing_ok=True)
            argv = ["/usr/bin/time", "-l", binary, "query", "--scmpp", str(scm), "--sqlite", str(db), *extra, str(path)]
            started = time.time()
            try:
                done = subprocess.run(argv, capture_output=True, text=True, timeout=int(os.environ.get("STRESS_TIMEOUT", "900")), env=dict(os.environ, RUST_LOG="warn"))
                code, err = done.returncode, done.stderr
            except subprocess.TimeoutExpired:
                code, err = -9, "wall timeout " + os.environ.get("STRESS_TIMEOUT", "900") + "s"
            wall = time.time() - started
            rss = re.search(r"(\d+)\s+maximum resident set size", err)
            counts = ["", "", "", ""]
            size = db.stat().st_size if db.exists() else ""
            if code == 0 and db.exists():
                con = sqlite3.connect(db)
                counts = [con.execute(f"select count(*) from {t}").fetchone()[0] for t in ("capture", "edge", "node", "scmpp_row")]
                con.close()
            message = next((l for l in err.splitlines() if "scm++" in l or "error" in l.lower() or "timeout" in l), "")
            row = [label, corpus, name, code, f"{wall:.2f}", rss.group(1) if rss else "", size, *counts, message[:160]]
            with out.open("a") as handle:
                handle.write("\t".join(map(str, row)) + "\n")
            print(*row, sep="\t", flush=True)
            db.unlink(missing_ok=True)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
