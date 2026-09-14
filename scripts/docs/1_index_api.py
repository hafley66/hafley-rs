#!/usr/bin/env python3
"""Write the API landing page for the docs site.

The page is derived from two sources: the workspace membership reported by
`cargo metadata`, and the rustdoc directories `cargo doc` actually emitted
under the target directory. A crate is listed as a link only when its target
emitted `index.html`; an expected target that did not emit is reported on
stdout and on the page as "not emitted" rather than dropped.

Links are document-relative, so the page works under any GitHub Pages base
path without a rewrite step.
"""

from __future__ import annotations

import argparse
import html
import json
import subprocess
import sys
from pathlib import Path

DOCUMENTED_KINDS = ("lib", "bin", "proc-macro")

KIND_LABEL = {
    "lib": "library",
    "bin": "binary",
    "proc-macro": "proc-macro",
}


def cargo_metadata(repo_root: Path) -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def documented_targets(metadata: dict) -> list[dict]:
    """Workspace packages, each with its documented lib/bin targets."""
    members = set(metadata.get("workspace_members", []))
    packages = []
    for package in metadata["packages"]:
        if package["id"] not in members:
            continue
        targets = [
            target
            for target in package["targets"]
            if any(kind in target["kind"] for kind in DOCUMENTED_KINDS)
            and target.get("doc", True)
        ]
        packages.append({"package": package, "targets": targets})
    packages.sort(key=lambda entry: entry["package"]["name"])
    return packages


def emitted_dirs(doc_dir: Path) -> set[str]:
    if not doc_dir.is_dir():
        return set()
    return {
        entry.name
        for entry in doc_dir.iterdir()
        if entry.is_dir() and (entry / "index.html").is_file()
    }


def build_page(packages: list[dict], doc_dir: Path) -> tuple[str, list[str]]:
    emitted = emitted_dirs(doc_dir)
    missing: list[str] = []
    rows: list[str] = []

    for entry in packages:
        package = entry["package"]
        targets = entry["targets"]
        if not targets:
            rows.append(
                "<tr><td>{name}</td><td>no documented targets</td>"
                "<td><span class=\"missing\">none</span></td></tr>".format(
                    name=html.escape(package["name"])
                )
            )
            continue
        for target in targets:
            name = target["name"]
            kind = "+".join(KIND_LABEL.get(k, k) for k in target["kind"] if k in KIND_LABEL)
            if name in emitted:
                cell = f'<a href="./{html.escape(name)}/index.html">{html.escape(name)}</a>'
            else:
                cell = f'<span class="missing">{html.escape(name)} (not emitted)</span>'
                missing.append(f"{package['name']} ({kind}): {name}")
            rows.append(
                "<tr><td>{package}</td><td>{kind}</td><td>{cell}</td></tr>".format(
                    package=html.escape(package["name"]),
                    kind=html.escape(kind),
                    cell=cell,
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
    return page, missing


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--doc-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    metadata = cargo_metadata(args.repo_root)
    packages = documented_targets(metadata)
    page, missing = build_page(packages, args.doc_dir)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(page, encoding="utf-8")

    total = sum(len(entry["targets"]) for entry in packages)
    print(f"api index: {len(packages)} crates, {total} documented targets -> {args.out}")
    for item in missing:
        print(f"api index: not emitted: {item}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
