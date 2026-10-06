"""Explicit Go module scaling check. Run alone: python3 bench/scripts/5_go_module_scaling.py /path/to/ryii."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

binary = Path(sys.argv[1]).resolve()
manifest = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='go-module-scaling-') as scratch:
    arguments = []
    for count in (200, 400):
        root = Path(scratch) / str(count)
        (root / 'leaf').mkdir(parents=True)
        (root / 'caller').mkdir()
        (root / 'go.mod').write_text('module example.com/gen\n\ngo 1.22\n')
        leaf = root / 'leaf/leaf.go'
        leaf.write_text('package leaf\n\n' + ''.join(f'func Pick{i}() int {{ return {i} }}\n' for i in range(count)))
        files = [leaf]
        for index in range(count):
            file = root / f'caller/f{index}.go'
            file.write_text(f'package caller\n\nimport "example.com/gen/leaf"\n\nfunc call{index}() int {{ return leaf.Pick{index}() }}\n')
            files.append(file)
        arguments.append(['--resolve', '--arms', 'call', *map(str, files)])
    walls = []
    for args in arguments:
        started = time.monotonic()
        result = subprocess.run([str(binary), *args], cwd=manifest, env=dict(os.environ, KACHE_DISABLED='1'), capture_output=True)
        walls.append(time.monotonic() - started)
        assert result.returncode == 0, result.stderr.decode()
    ratio = walls[1] / walls[0]
    print(json.dumps(dict(wall200=walls[0], wall400=walls[1], ratio=ratio, budget=2.5)))
    assert ratio < 2.5, f'Go module scaling ratio {ratio} >= 2.5'
