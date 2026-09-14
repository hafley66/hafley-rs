#!/usr/bin/env python3
"""Coverage and link checker for the assembled docs site.

Coverage: every workspace crate's documented lib/bin target must have emitted
`index.html` under the rustdoc directory, and that target must be linked from
`api/index.html`. Targets that cargo does not document (examples, integration
tests, build scripts, anything with `doc = false`) are listed in the report so
an exclusion is visible, never silent.

Links: internal href/src values in the book's HTML pages and in the generated
API landing page must resolve to a file in the site. A leading `/` is reported
as a base-path failure, because the site is served from `/hafley-rs/`.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlsplit

DOCUMENTED_KINDS = ("lib", "bin", "proc-macro")
LINK_ATTR = re.compile(r'(?:href|src)\s*=\s*"([^"]*)"')
SKIP_SCHEMES = ("http", "https", "mailto", "tel", "data", "javascript")


def cargo_metadata(repo_root: Path) -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def workspace_packages(metadata: dict) -> list[dict]:
    members = set(metadata.get("workspace_members", []))
    return sorted(
        (p for p in metadata["packages"] if p["id"] in members),
        key=lambda package: package["name"],
    )


def check_coverage(metadata: dict, doc_dir: Path, api_index: Path) -> tuple[list[str], list[str]]:
    failures: list[str] = []
    exclusions: list[str] = []
    index_text = api_index.read_text(encoding="utf-8") if api_index.is_file() else ""

    if not api_index.is_file():
        failures.append("api/index.html: missing (index generator did not run)")

    for package in workspace_packages(metadata):
        documented = [
            target
            for target in package["targets"]
            if any(kind in target["kind"] for kind in DOCUMENTED_KINDS)
            and target.get("doc", True)
        ]
        for target in package["targets"]:
            if target in documented:
                continue
            kind = "+".join(target["kind"])
            exclusions.append(f"{package['name']} ({kind}): {target['name']}")

        if not documented:
            failures.append(f"{package['name']}: no documented lib/bin target")
            continue

        for target in documented:
            name = target["name"]
            page = doc_dir / name / "index.html"
            if not page.is_file():
                failures.append(f"{package['name']}: rustdoc missing at {page}")
                continue
            marker = f'href="./{name}/index.html"'
            if marker not in index_text:
                failures.append(f"{package['name']}: api/index.html does not link {name}")
    return failures, exclusions


def check_links(files: list[Path], site_dir: Path, base_path: str) -> list[str]:
    failures: list[str] = []
    root = "/" + base_path.strip("/") + "/"
    for page in files:
        text = page.read_text(encoding="utf-8", errors="replace")
        for raw in LINK_ATTR.findall(text):
            if not raw or raw.startswith("#"):
                continue
            parts = urlsplit(raw)
            if parts.scheme in SKIP_SCHEMES or parts.netloc:
                continue
            target = parts.path
            if not target:
                continue
            if target.startswith("/"):
                # An absolute link is only sound when it stays inside the
                # project Pages base path. Everything the book and the landing
                # page emit is document-relative; this catches the rest.
                if target.rstrip("/") == root.rstrip("/"):
                    relative = ""
                elif target.startswith(root):
                    relative = target[len(root):]
                else:
                    failures.append(
                        f"{page.relative_to(site_dir)}: absolute link {raw!r} "
                        f"outside base path {root!r}"
                    )
                    continue
                resolved = (site_dir / relative).resolve()
            else:
                resolved = (page.parent / target).resolve()
            try:
                resolved.relative_to(site_dir.resolve())
            except ValueError:
                failures.append(f"{page.relative_to(site_dir)}: link escapes site {raw!r}")
                continue
            if not resolved.exists():
                failures.append(f"{page.relative_to(site_dir)}: broken link {raw!r}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--site-dir", type=Path, required=True)
    parser.add_argument("--doc-dir", type=Path, required=True)
    parser.add_argument(
        "--base-path",
        default="/hafley-rs/",
        help="GitHub Pages project base path; absolute links must stay inside it",
    )
    args = parser.parse_args()

    metadata = cargo_metadata(args.repo_root)
    api_index = args.site_dir / "api" / "index.html"

    failures, exclusions = check_coverage(metadata, args.doc_dir, api_index)

    if not (args.site_dir / "index.html").is_file():
        failures.append("index.html: missing (book root did not build)")

    scanned = sorted(args.site_dir.glob("*.html"))
    if api_index.is_file():
        scanned.append(api_index)
    failures.extend(check_links(scanned, args.site_dir, args.base_path))

    print(f"coverage: {len(scanned)} pages scanned, {len(exclusions)} undocumented targets")
    for item in exclusions:
        print(f"  not documented: {item}")
    if failures:
        print(f"\n{len(failures)} failure(s):", file=sys.stderr)
        for item in failures:
            print(f"  {item}", file=sys.stderr)
        return 1
    print("check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
