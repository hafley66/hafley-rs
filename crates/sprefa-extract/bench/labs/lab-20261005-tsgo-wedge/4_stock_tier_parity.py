#!/usr/bin/env python3
"""One same-root capture per tier; compare semantic relation counts.

Pass --old-capture to reuse a recorded TS5 run. No installs or builds.
"""
import argparse
import collections
import json
import os
import pathlib
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--ryii', required=True, type=pathlib.Path)
parser.add_argument('--typescript5', type=pathlib.Path,
                    default=pathlib.Path.home() / 'projects/TypeScript-5.9/lib/typescript.js')
parser.add_argument('--old-capture', type=pathlib.Path)
parser.add_argument('--new-capture', type=pathlib.Path)
args = parser.parse_args()
crate = pathlib.Path(__file__).resolve().parents[3]
repo = crate.parents[1]
fixture = 'tests/fixtures/tsi/probe.ts'
root = (crate / 'tests/fixtures/tsi').resolve()
if args.old_capture:
    old = args.old_capture.read_text()
else:
    script = subprocess.check_output([
        'git', 'show', '9d4f0dc6b86d944461d6c47ad4673d8d25f938b2:crates/hafley_scm/src/read/lang/ts_checker.mjs'
    ], cwd=repo)
    with tempfile.TemporaryDirectory(prefix='stock-tsgo-parity-') as directory:
        directory = pathlib.Path(directory)
        driver = directory / 'ts_checker.mjs'
        driver.write_bytes(script)
        request = directory / 'request.json'
        request.write_text(json.dumps({'root': str(root),
            'files': [[fixture, str(crate / fixture)]], 'tsi': True}))
        old = subprocess.check_output(['node', str(driver), str(request)], cwd=crate,
            env={**os.environ, 'SPREFA_TS_CHECKER_TYPESCRIPT': str(args.typescript5)},
            text=True, timeout=90)
start = time.monotonic()
new = subprocess.check_output([str(args.ryii.resolve()), '--witness', '--resolve',
    '--arms', 'type', '--root', 'tests/fixtures/tsi', '--ts-checker', fixture],
    cwd=crate, text=True, timeout=90)
if args.new_capture:
    args.new_capture.write_text(new)
old_counts = collections.Counter(row[0] for line in old.splitlines()
    for row in json.loads(line).get('tsi', []))
rows = [json.loads(line) for line in new.splitlines()]
semantic = {row['fact'] for row in rows if row['record'] == 'witness'
    and row['method'] == 'checker_walk'}
new_counts = collections.Counter(row['relation'] for row in rows
    if row['record'] == 'fact' and row['fact'] in semantic)
assert any(row['record'] == 'run' and row.get('tool') == 'tsgo'
    and row.get('version') == '7.0.2' for row in rows)
print('relation\trows_old\trows_new')
for relation in sorted(old_counts.keys() | new_counts.keys()):
    print(f'{relation}\t{old_counts[relation]}\t{new_counts[relation]}')
print(f'TOTAL\t{sum(old_counts.values())}\t{sum(new_counts.values())}')
print(f'new_wall_s={time.monotonic() - start:.3f}', file=__import__('sys').stderr)
