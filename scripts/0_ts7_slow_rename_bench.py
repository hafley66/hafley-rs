"""Run TS7 slow rename against each TypeScript truth target on a fresh corpus copy."""

import argparse
import difflib
import json
import shutil
import sqlite3
import subprocess
from pathlib import Path


BENCH = Path.home() / ".cache/lanes/claude-375/eval/bench"
WORK = Path.home() / ".cache/lanes/claude-375/eval/bench/slow-rename-work"
TSC = Path(__file__).resolve().parents[1] / "crates/sprefa-extract/ts7/node_modules/typescript/bin/tsc"
RYII = Path.home() / ".cache/lanes/claude-375/target-writer/debug/ryii"


def typecheck(root, anchor):
    project = next((p / "tsconfig.json" for p in (anchor.parent, *anchor.parents)
                    if (p / "tsconfig.json").is_file() and p.is_relative_to(root)), None)
    command = [str(TSC), "--noEmit"]
    command += ["--project", str(project)] if project else [str(anchor)]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True)
    return sorted(set((result.stdout + result.stderr).splitlines()))


def changed_sites(before, after, name):
    changes = set()
    for rel, old in before.items():
        new = after.get(rel)
        if new is None or new == old:
            continue
        old_lines = old.splitlines()
        new_lines = new.splitlines()
        for tag, a, b, _, _ in difflib.SequenceMatcher(None, old_lines, new_lines).get_opcodes():
            if tag != "equal":
                changes.update((rel, line + 1) for line in range(a, b))
    return changes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()
    truth = sqlite3.connect(BENCH / "truth.db")
    rows = truth.execute("""select repo,symbol,name,def_path,def_line from target
        where repo in ('vite','codegraph-src') and (def_path like '%.ts' or def_path like '%.tsx')
        order by repo,def_path,def_line,name""").fetchall()
    if args.limit:
        rows = rows[:args.limit]
    WORK.mkdir(parents=True, exist_ok=True)
    report = []
    for number, (repo, symbol, name, path, def_line) in enumerate(rows):
        source = BENCH / "runs/ryii" / repo
        root = WORK / f"{repo}-{number:03}"
        shutil.rmtree(root, ignore_errors=True)
        shutil.copytree(source, root, ignore=shutil.ignore_patterns(".git", "index.scip"))
        anchor = root / path
        old = name.removesuffix(":").rsplit(":", 1)[-1]
        lines = anchor.read_text().splitlines()
        line = lines[def_line - 1] if def_line <= len(lines) else ""
        at = sum(len(part.encode()) + 1 for part in lines[:def_line - 1]) + line.find(old) if old in line else None
        baseline = typecheck(root, anchor)
        originals = {str(p.relative_to(root)): p.read_text() for p in root.rglob("*.ts*") if p.is_file()}
        command = [str(RYII), "rename", f"{path}#{old}", f"{old}_zz", "--root", str(root), "--slow", "--commit", "--json"]
        if at is not None:
            command += ["--at", str(at)]
        measured = subprocess.run(["python3", str(BENCH / "run.py"), repo, "ryi", "slow_rename", symbol,
                                   "--", *command], capture_output=True, text=True)
        after = typecheck(root, anchor)
        changed = changed_sites(originals, {str(p.relative_to(root)): p.read_text()
                                            for p in root.rglob("*.ts*") if p.is_file()}, old)
        truth_sites = set(truth.execute("select path,line from occ where repo=? and symbol=?", (repo, symbol)))
        run = sqlite3.connect(BENCH / "bench.db").execute(
            "select rc,wall_s,out_path from run where repo=? and tool='ryi' and endpoint='slow_rename' and arg=?",
            (repo, symbol)).fetchone()
        result = {"repo": repo, "symbol": symbol, "name": name, "path": path,
                  "return_code": run[0] if run else None, "wall_seconds": run[1] if run else None,
                  "baseline_errors": len(baseline), "after_errors": len(after),
                  "typecheck_pass": baseline == after, "changed_sites": sorted(changed),
                  "truth_sites": sorted(truth_sites), "diff_isolated": bool(changed) and changed <= truth_sites,
                  "file_recall": len({p for p, _ in changed} & {p for p, _ in truth_sites}) / len({p for p, _ in truth_sites}) if truth_sites else None,
                  "site_recall": len(changed & truth_sites) / len(truth_sites) if truth_sites else None,
                  "harness": measured.stdout.strip(), "harness_error": measured.stderr.strip()}
        report.append(result)
        (WORK / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{number + 1}/{len(rows)} {repo} {name} rc={result['return_code']} "
              f"typecheck={result['typecheck_pass']} isolated={result['diff_isolated']}", flush=True)
        shutil.rmtree(root)
    print(WORK / "results.json")


if __name__ == "__main__":
    main()
