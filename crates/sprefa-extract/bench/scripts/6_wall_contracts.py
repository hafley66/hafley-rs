"""Run clock contracts sequentially: python3 bench/scripts/6_wall_contracts.py /path/to/all-test-binary [--only substring] [--include-local]."""
import argparse
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('--only', default='')
parser.add_argument('--include-local', action='store_true')
parser.add_argument('--ratios-only', action='store_true')
args = parser.parse_args()
manifest = Path(__file__).resolve().parents[2]
rows = json.loads((manifest / 'bench/fixtures/wall_contracts/0_rows.json').read_text())
selected = [row for row in rows if args.only in row['test'] and (args.include_local or not row['local']) and (not args.ratios_only or 'wall400 / wall200' in row['condition'] or 'wall200 / wall100' in row['condition'])]
assert selected, 'no matching benchmark rows'
for row in selected:
    print(json.dumps(row), flush=True)
    result = subprocess.run([str(args.binary.resolve()), '--exact', row['test'], '--include-ignored', '--test-threads=1', '--nocapture'], cwd=manifest, env=dict(os.environ, KACHE_DISABLED='1', SPREFA_WALL_BENCH='1'), capture_output=True, text=True)
    print(result.stdout, end='', flush=True)
    print(result.stderr, end='', flush=True)
    assert result.returncode == 0, row['test']
    assert '1 passed;' in result.stdout, 'benchmark target was not compiled: ' + row['test']
