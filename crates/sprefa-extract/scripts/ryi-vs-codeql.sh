#!/usr/bin/env bash
# usage: ryi-vs-codeql.sh <root> <rust|ts>
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <root> <rust|ts>" >&2
  exit 2
fi
source_root=$(cd "$1" && pwd -P)
case "$2" in
  rust) language=rust; query_dir=$(cd "$(dirname "$0")/../codeql" && pwd -P) ;;
  ts|typescript) language=javascript; query_dir=$(cd "$(dirname "$0")/../codeql/javascript" && pwd -P) ;;
  *) echo "language must be rust or ts" >&2; exit 2 ;;
esac
if [ -n "${CODEQL_BIN:-}" ]; then
  codeql=$CODEQL_BIN
elif [ -x "$HOME/.local/bin/codeql" ]; then
  codeql=$HOME/.local/bin/codeql
else
  codeql=$(command -v codeql)
fi
ryi=${RYI_BIN:-$(command -v ryi)}
key=$(printf '%s\n%s' "$source_root" "$language" | shasum -a 256 | cut -c1-16)
out=${RYI_CODEQL_OUT:-${XDG_CACHE_HOME:-$HOME/.cache}/ryi-vs-codeql/$key}
mkdir -p "$out"
cleanup() {
  rm -rf "$out/codeql-db" "$out/source" "$out/dependency-overrides"
  rm -f "$out/ryi.db"
}
trap cleanup EXIT

# Both extractors see the same source snapshot. In a Cargo workspace, excluded
# member directories are left out of that snapshot.
if [ "${RYI_CODEQL_STAGE:-1}" = 0 ]; then
  root=$source_root
elif [ "${RYI_CODEQL_REUSE:-0}" != 1 ] || [ ! -f "$out/source/.baseline-complete" ]; then
  rm -rf "$out/source"
  python3 - "$source_root" "$out/source" "$language" <<'PY'
import pathlib
import shutil
import subprocess
import sys
import tomllib

source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
language = sys.argv[3]
manifest = source / 'Cargo.toml'
excluded = ()
if language == 'rust' and manifest.is_file():
    excluded = tuple(pathlib.PurePosixPath(p).parts for p in
                     tomllib.loads(manifest.read_text()).get('workspace', {}).get('exclude', []))
files = subprocess.check_output(['rg', '--files', '--hidden', str(source)], text=True).splitlines()
count = 0
for filename in files:
    path = pathlib.Path(filename)
    relative = path.relative_to(source)
    if any(relative.parts[:len(prefix)] == prefix for prefix in excluded):
        continue
    if language == 'rust':
        keep = path.suffix == '.rs' or path.name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')
    else:
        keep = path.suffix in ('.ts', '.tsx') or path.name == 'package.json' or path.name.startswith(('tsconfig', 'jsconfig')) and path.suffix == '.json'
    if not keep:
        continue
    dest = target / relative
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, dest)
    if path.suffix in ('.rs', '.ts', '.tsx'):
        count += 1
(target / '.baseline-complete').write_text(str(count) + '\n')

# ra_ap_parser's intentionally invalid lexer fixture starts with "-│". CodeQL
# parses dependency test_data and panics on that byte boundary. Patch only a
# private dependency copy; the repository and Cargo registry stay untouched.
if language == 'rust' and (source / 'crates/boop').is_dir() and (source / 'Cargo.lock').is_file():
    packages = tomllib.loads((source / 'Cargo.lock').read_text()).get('package', [])
    versions = [p['version'] for p in packages if p['name'] == 'ra_ap_parser']
    if versions:
        version = versions[0]
        matches = list((pathlib.Path.home() / '.cargo/registry/src').glob(f'*/ra_ap_parser-{version}'))
        if not matches:
            raise FileNotFoundError(f'Cargo registry lacks ra_ap_parser {version}')
        dep = target.parent / 'dependency-overrides' / f'ra_ap_parser-{version}'
        shutil.copytree(matches[0], dep, dirs_exist_ok=True)
        fixture = dep / 'test_data/lexer/err/incomplete_frontmatter_before_unicode.rs'
        if fixture.is_file():
            fixture.write_text(fixture.read_text().replace('│', '|'))
            patch = target / 'Cargo.toml'
            header = '' if '[patch.crates-io]' in patch.read_text() else '\n[patch.crates-io]\n'
            patch.write_text(patch.read_text() + header +
                             f'ra_ap_parser = {{ path = "../dependency-overrides/ra_ap_parser-{version}" }}\n')
            (target / '.dependency-rewrite').write_text(str(fixture) + '\n')
PY
fi
if [ "${RYI_CODEQL_STAGE:-1}" != 0 ]; then
  root=$(cd "$out/source" && pwd -P)
fi

if [ "${RYI_CODEQL_REUSE:-0}" != 1 ] || [ ! -f "$out/ryi.db" ]; then
  rm -f "$out/ryi.db"
  start=$(date +%s)
  RYI_MAX_MEM_MB=2048 RUST_LOG=off "$ryi" fast "$root" --sqlite "$out/ryi.db" >"$out/ryi.log" 2>&1
  echo "$(( $(date +%s) - start ))" >"$out/ryi-build-seconds"
fi

if [ "${RYI_CODEQL_REUSE:-0}" != 1 ] || [ ! -d "$out/codeql-db" ]; then
  rm -rf "$out/codeql-db"
  start=$(date +%s)
  cargo_target_option=()
  if [ -n "${RYI_CODEQL_CARGO_TARGET_DIR:-}" ]; then
    cargo_target_option=("--extractor-option=rust.cargo_target_dir=$RYI_CODEQL_CARGO_TARGET_DIR")
  fi
  if ! CARGO_BUILD_JOBS=4 "$codeql" database create "$out/codeql-db" -l "$language" --source-root "$root" \
      "${cargo_target_option[@]}" \
      --ram=2048 --threads=4 -J=-Xmx2g >"$out/codeql-build.log" 2>&1; then
    echo "CodeQL database creation failed; log: $out/codeql-build.log" >&2
    rg -n -C 2 'panicked|panic|│|ERROR|Error' "$out/codeql-build.log" | tail -40 >&2 || true
    exit 1
  fi
  echo "$(( $(date +%s) - start ))" >"$out/codeql-build-seconds"
fi

start=$(date +%s)
query_ran=0
for kind in type call; do
  if [ "${RYI_CODEQL_REUSE_QUERIES:-0}" = 1 ] && [ -f "$out/$kind.csv" ]; then
    continue
  fi
  query_ran=1
  "$codeql" query run "$query_dir/${kind}_edges_repo.ql" \
    --database "$out/codeql-db" --output "$out/$kind.bqrs" \
    --ram=2048 --threads=4 -J=-Xmx2g \
    >"$out/$kind-query.log" 2>&1
  "$codeql" bqrs decode "$out/$kind.bqrs" --format=csv --output "$out/$kind.csv" -J=-Xmx2g \
    >>"$out/$kind-query.log" 2>&1
done
if [ "$query_ran" = 1 ] || [ ! -f "$out/codeql-query-seconds" ]; then
  echo "$(( $(date +%s) - start ))" >"$out/codeql-query-seconds"
fi

python3 - "$root" "$out" <<'PY'
import csv
import pathlib
import sqlite3
import sys
import time

started = time.perf_counter()
root = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
conn = sqlite3.connect(out / 'ryi.db')

def path(value):
    if not value:
        return None
    p = pathlib.Path(value)
    if p.is_absolute():
        p = p.resolve()
        try:
            return p.relative_to(root).as_posix()
        except ValueError:
            return None
    text = p.as_posix()
    if text.startswith('./'):
        text = text[2:]
    return text if not text.startswith('../') else None

summary = []
disagreements = []
for kind, table, cols in (
    ('type', 'resolved_type_edge', ('owner_path', 'owner_name', 'target_path', 'target_name')),
    ('call', 'resolved_edge', ('caller_path', 'caller_name', 'callee_path', 'callee_name')),
):
    sql = 'select distinct ' + ', '.join(cols) + ' from ' + table
    ryi = {(path(a), b, path(c), d) for a, b, c, d in conn.execute(sql)}
    with (out / f'{kind}.csv').open(newline='') as stream:
        rows = csv.reader(stream)
        next(rows)
        codeql = {(path(a), b, path(c), d) for a, b, c, d in rows}
    ryi = {row for row in ryi if all(row)}
    codeql = {row for row in codeql if all(row)}
    both = ryi & codeql
    only_ryi = ryi - codeql
    only_codeql = codeql - ryi
    summary.append((kind, len(both), len(only_ryi), len(only_codeql)))
    for bucket, rows in (('ryi-only', only_ryi), ('codeql-only', only_codeql)):
        disagreements.extend((kind, bucket, *row, '') for row in sorted(rows))

with (out / 'disagreements.tsv').open('w', newline='') as stream:
    writer = csv.writer(stream, delimiter='\t')
    writer.writerow(('kind', 'bucket', 'src_file', 'enclosing_item', 'dst_file', 'dst_name', 'verdict'))
    writer.writerows(disagreements)
with (out / 'summary.tsv').open('w', newline='') as stream:
    writer = csv.writer(stream, delimiter='\t')
    writer.writerow(('kind', 'agree', 'ryi-only', 'codeql-only'))
    writer.writerows(summary)

print('kind\tagree\tryi-only\tcodeql-only')
for row in summary:
    print(*row, sep='\t')
print('sample\tkind\tbucket\tsrc_file\tenclosing_item\tdst_file\tdst_name\tverdict')
for kind in ('type', 'call'):
    for bucket in ('ryi-only', 'codeql-only'):
        sample = [row for row in disagreements if row[0] == kind and row[1] == bucket][:20]
        for row in sample:
            print('sample', *row, sep='\t')
print(f'full_disagreements\t{out / "disagreements.tsv"}')
source_count = sum(p.suffix in ('.rs', '.ts', '.tsx') for p in root.rglob('*') if p.is_file())
print(f'source_files\t{source_count}')
rewrite = out / 'source/.dependency-rewrite'
if rewrite.is_file():
    print(f'dependency_rewrite\t{rewrite.read_text().strip()}')
print(f'ryi_build_s\t{(out / "ryi-build-seconds").read_text().strip()}')
print(f'ryi_query_s\t{time.perf_counter() - started:.3f}')
print(f'codeql_build_s\t{(out / "codeql-build-seconds").read_text().strip()}')
print(f'codeql_query_s\t{(out / "codeql-query-seconds").read_text().strip()}')
print(f'ryi_db_kib\t{(out / "ryi.db").stat().st_size // 1024}')
database = out / 'codeql-db'
content = [*database.glob('db-*'), database / 'src.zip']
print(f'codeql_db_kib\t{sum(p.stat().st_size for root in content if root.exists() for p in root.rglob("*") if p.is_file()) // 1024 + (database / "src.zip").stat().st_size // 1024}')
PY
