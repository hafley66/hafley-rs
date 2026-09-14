#!/usr/bin/env python3
"""Coverage and link checker for the assembled docs site.

Coverage follows the same rules as `cargo doc --workspace --no-deps --locked`
with default features (see `cargo_targets`):

- every lib/bin/proc-macro target with `doc != false` and satisfied
  `required-features` must have emitted `target/doc/<name>/index.html`, and that
  page must be linked from `api/index.html`;
- rustdoc output directories use underscores for hyphens;
- a library and binary sharing a name emit one directory, so existence and the
  landing link are keyed by directory;
- every other target is reported with its reason (not a lib/bin target,
  `doc = false`, or an unmet `required-features`) so a gap is visible, never
  silent.

Links: internal href/src values in the book's HTML pages and in the generated
API landing page must resolve to a file in the site. An absolute link is
accepted only when it stays inside the configured Pages base path.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import urlsplit

import cargo_targets as ct

LINK_ATTR = re.compile(r'(?:href|src)\s*=\s*"([^"]*)"')
SKIP_SCHEMES = ("http", "https", "mailto", "tel", "data", "javascript")


def check_coverage(
    metadata: dict, doc_dir: Path, api_index: Path
) -> tuple[list[str], list[str], int]:
    failures: list[str] = []
    exclusions: list[str] = []
    documented_total = 0
    index_text = api_index.read_text(encoding="utf-8") if api_index.is_file() else ""

    if not api_index.is_file():
        failures.append("api/index.html: missing (index generator did not run)")

    for package in ct.workspace_packages(metadata):
        name = package["name"]
        documented, excluded = ct.classify_targets(package)
        documented_total += len(documented)
        for target, reason in excluded:
            kind = "+".join(target["kind"])
            exclusions.append(f"{name} ({kind}): {target['name']} [{reason}]")

        if not documented:
            failures.append(f"{name}: no documented lib/bin target with default features")
            continue

        seen_dirs: set[str] = set()
        for target in documented:
            directory = ct.emitted_dir_name(target)
            page = doc_dir / directory / "index.html"
            if directory not in seen_dirs:
                seen_dirs.add(directory)
                if not page.is_file():
                    failures.append(f"{name}: rustdoc missing at {page}")
                marker = f'href="./{directory}/index.html"'
                if marker not in index_text:
                    failures.append(f"{name}: api/index.html does not link {directory}")

    return failures, exclusions, documented_total


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

    metadata = ct.cargo_metadata(args.repo_root)
    api_index = args.site_dir / "api" / "index.html"

    failures, exclusions, documented_total = check_coverage(
        metadata, args.doc_dir, api_index
    )

    if not (args.site_dir / "index.html").is_file():
        failures.append("index.html: missing (book root did not build)")

    scanned = sorted(args.site_dir.glob("*.html"))
    if api_index.is_file():
        scanned.append(api_index)
    failures.extend(check_links(scanned, args.site_dir, args.base_path))

    print(
        f"coverage: {documented_total} documented targets expected, "
        f"{len(scanned)} pages scanned, {len(exclusions)} targets not documented"
    )
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
