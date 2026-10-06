"""Single runs of the 2026-10-02 ancestor stress queries on frozen corpora.

Usage: python3 0_measure.py BINARY LABEL RUST_CORPUS TS_CORPUS
Writes TSV to stdout. Corpus budgets are 60s; deep each has a 10s budget.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

lab = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "stress", lab.parent / "lab-20261002-ryi-scmpp-stress/4_stress.py"
)
stress = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stress)
binary, label, rust, ts = sys.argv[1:]

with tempfile.TemporaryDirectory() as temporary:
    scm = Path(temporary) / "query.scm"
    cases = []
    for language, corpus, patterns in [
        ("rust", Path(rust), ["*.rs"]),
        ("ts", Path(ts), ["*.ts", "*.tsx"]),
    ]:
        files = sorted(p for p in corpus.rglob("*") if p.is_file()
                       and any(p.match(pattern) for pattern in patterns))
        digest = hashlib.sha256()
        for file in files:
            digest.update(str(file.relative_to(corpus)).encode())
            digest.update(b"\0")
            digest.update(file.read_bytes())
        extra = [arg for pattern in patterns for arg in ("--pattern", pattern)]
        for name in ["ancestor_fn", "ancestor_each", "nested3_each"]:
            cases.append((language, name, corpus, extra, stress.QUERIES[name][language],
                          60, len(files), digest.hexdigest()))
    deep = Path(temporary) / "deep.rs"
    deep.write_text("fn d() { " + "{ " * 200 + "g(); " + "} " * 200 + "}\n")
    for name, levels, each in [("deep5_first", 5, False),
                               ("deep20_first", 20, False), ("deep5_each", 5, True)]:
        query = "(block)"
        for level in range(levels):
            query = f"((block) @b{level} (#has-ancestor? @b{level} {query}" + (" rows: each" if each else "") + "))"
        cases.append(("rust", name, deep, [], query, 10, 1,
                      hashlib.sha256(deep.read_bytes()).hexdigest()))
    print("label\tlanguage\tquery\tfiles\tcorpus_sha256\texit\tseconds\trows\trows_sha256\terror", flush=True)
    for language, name, corpus, extra, query, budget, files, digest in cases:
        scm.write_text(query)
        started = time.monotonic()
        try:
            done = subprocess.run([binary, "query", "--timeout", str(budget), "--scmpp", str(scm),
                                   *extra, str(corpus)], capture_output=True, text=True,
                                  timeout=budget + 20, env=dict(os.environ, DL_TRAIL="0", RUST_LOG="off"))
            code, stdout, stderr = done.returncode, done.stdout, done.stderr.strip()
        except subprocess.TimeoutExpired:
            code, stdout, stderr = -9, "", "external timeout"
        normalized = stdout.replace(str(corpus), "corpus")
        print(label, language, name, files, digest, code, f"{time.monotonic() - started:.3f}",
              len(stdout.splitlines()), hashlib.sha256(normalized.encode()).hexdigest(),
              json.dumps(stderr), sep="\t", flush=True)
