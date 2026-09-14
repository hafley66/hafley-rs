#!/usr/bin/env python3
"""Write the API landing page for the docs site.

The page is derived from two sources: the workspace membership reported by
`cargo metadata`, and the rustdoc directories `cargo doc` actually emitted
under the target directory. Only targets `cargo doc --workspace --no-deps
--locked` documents with default features are expected (see `cargo_targets`).
A crate is linked only when its emitted directory has an `index.html`; an
expected page that did not emit is reported on stdout and on the page as
"not emitted" rather than dropped.

A library and binary that share a name emit one directory, so rows are keyed by
emitted directory name and show the combined kind.

Links are document-relative, so the page works under any GitHub Pages base
path without a rewrite step.
"""

from __future__ import annotations

import argparse
import html
import sys
from pathlib import Path

import cargo_targets as ct


def emitted_dirs(doc_dir: Path) -> set[str]:
    if not doc_dir.is_dir():
        return set()
    return {
        entry.name
        for entry in doc_dir.iterdir()
        if entry.is_dir() and (entry / "index.html").is_file()
    }


def build_page(metadata: dict, doc_dir: Path) -> tuple[str, int, list[str], int]:
    emitted = emitted_dirs(doc_dir)
    missing: list[str] = []
    rows: list[str] = []
    targets_total = 0
    pages_total = 0

    for package in ct.workspace_packages(metadata):
        name = package["name"]
        documented, _ = ct.classify_targets(package)
        targets_total += len(documented)
        if not documented:
            rows.append(
                "<tr><td>{name}</td><td>no documented targets</td>"
                "<td><span class=\"missing\">none</span></td></tr>".format(
                    name=html.escape(name)
                )
            )
            continue

        by_dir: dict[str, list[dict]] = {}
        for target in documented:
            by_dir.setdefault(ct.emitted_dir_name(target), []).append(target)

        for directory in sorted(by_dir):
            pages_total += 1
            kinds = "+".join(
                dict.fromkeys(
                    ct.target_kind_label(target) for target in by_dir[directory]
                )
            )
            if directory in emitted:
                cell = f'<a href="./{html.escape(directory)}/index.html">{html.escape(directory)}</a>'
            else:
                cell = f'<span class="missing">{html.escape(directory)} (not emitted)</span>'
                missing.append(f"{name} ({kinds}): {directory}")
            rows.append(
                "<tr><td>{package}</td><td>{kind}</td><td>{cell}</td></tr>".format(
                    package=html.escape(name), kind=html.escape(kinds), cell=cell
                )
            )

    page = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>hafley-rs API reference</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  margin: 2rem auto; max-width: 60rem; padding: 0 1rem; line-height: 1.5; }}
h1 {{ margin-bottom: 0.25rem; }}
p.lead {{ color: #555; margin-top: 0; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid #ddd; }}
th {{ border-bottom: 2px solid #999; }}
tr:hover td {{ background: #f6f6f6; }}
.missing {{ color: #a33; }}
code {{ background: #f2f2f2; padding: 0.1rem 0.25rem; border-radius: 3px; }}
</style>
</head>
<body>
<h1>hafley-rs API reference</h1>
<p class="lead">Rustdoc for the workspace crates, generated with
<code>cargo doc --workspace --no-deps --locked</code>.</p>
<table>
<thead><tr><th>crate</th><th>target</th><th>rustdoc</th></tr></thead>
<tbody>
{chr(10).join(rows)}
</tbody>
</table>
</body>
</html>
"""
    return page, pages_total, missing, targets_total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--doc-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    metadata = ct.cargo_metadata(args.repo_root)
    page, pages, missing, targets = build_page(metadata, args.doc_dir)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(page, encoding="utf-8")

    print(
        f"api index: {pages} rustdoc pages for {targets} documented targets "
        f"-> {args.out}"
    )
    for item in missing:
        print(f"api index: not emitted: {item}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
