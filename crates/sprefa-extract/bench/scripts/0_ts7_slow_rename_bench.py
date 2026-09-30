"""Run TS7 slow rename against each TypeScript truth target on a fresh corpus copy."""

import argparse
import difflib
import hashlib
import json
import shutil
import sqlite3
import subprocess
import time
from pathlib import Path


BENCH = Path.home() / ".cache/lanes/claude-375/eval/bench"
WORK = Path.home() / ".cache/lanes/claude-375/eval/bench/slow-rename-work"
TSC = Path(__file__).resolve().parents[4] / "crates/sprefa-extract/ts7/node_modules/typescript/bin/tsc"
RYII = Path.home() / ".cache/lanes/claude-375/target-writer/debug/ryii"


def measured_run(repo, symbol, command):
    key = hashlib.sha1(json.dumps([repo, "ryi", "slow_rename", symbol]).encode()).hexdigest()[:12]
    out_path = BENCH / "out" / f"{repo}.ryi.slow_rename.{key}.out"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    result = subprocess.run(command, capture_output=True)
    wall = round(time.monotonic() - started, 3)
    out_path.write_bytes(result.stdout)
    out_path.with_suffix(out_path.suffix + ".err").write_bytes(result.stderr)
    with sqlite3.connect(BENCH / "bench.db", timeout=120) as bench:
        bench.execute("""insert or replace into run values
            (?,?,?,?,?,?,?,?,?,?,?,datetime('now'))""", (
            repo, "ryi", "slow_rename", symbol, json.dumps(command), result.returncode,
            wall, len(result.stdout), 1, None, str(out_path),
        ))
    return result.returncode, wall, result.stdout.decode(errors="replace"), result.stderr.decode(errors="replace")


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
    parser.add_argument("--name")
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args()
    truth = sqlite3.connect(BENCH / "truth.db")
    rows = truth.execute("""select repo,symbol,name,def_path,def_line from target
        where repo in ('vite','codegraph-src') and (def_path like '%.ts' or def_path like '%.tsx')
        order by repo,def_path,def_line,name""").fetchall()
    if args.name:
        rows = [row for row in rows if row[2] == args.name]
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
        rc, wall, stdout, stderr = measured_run(repo, symbol, command)
        after = typecheck(root, anchor)
        changed = changed_sites(originals, {str(p.relative_to(root)): p.read_text()
                                            for p in root.rglob("*.ts*") if p.is_file()}, old)
        truth_sites = set(truth.execute("select path,line from occ where repo=? and symbol=?", (repo, symbol)))
        result = {"repo": repo, "symbol": symbol, "name": name, "path": path,
                  "return_code": rc, "wall_seconds": wall,
                  "baseline_errors": len(baseline), "after_errors": len(after),
                  "typecheck_pass": baseline == after, "changed_sites": sorted(changed),
                  "added_typecheck_errors": sorted(set(after) - set(baseline)),
                  "removed_typecheck_errors": sorted(set(baseline) - set(after)),
                  "truth_sites": sorted(truth_sites), "diff_isolated": bool(changed) and changed <= truth_sites,
                  "file_recall": len({p for p, _ in changed} & {p for p, _ in truth_sites}) / len({p for p, _ in truth_sites}) if truth_sites else None,
                  "site_recall": len(changed & truth_sites) / len(truth_sites) if truth_sites else None,
                  "harness": stdout.strip(), "harness_error": stderr.strip()}
        report.append(result)
        (WORK / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{number + 1}/{len(rows)} {repo} {name} rc={result['return_code']} "
              f"typecheck={result['typecheck_pass']} isolated={result['diff_isolated']}", flush=True)
        if not args.keep:
            shutil.rmtree(root)
    print(WORK / "results.json")


if __name__ == "__main__":
    main()
