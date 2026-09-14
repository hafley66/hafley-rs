#!/usr/bin/env python3
"""Guard the generated-site destination before anything deletes it.

`0_build_site.sh` wipes the destination with `rm -rf`, so the destination must
never be source, a repository root or ancestor, the rustdoc input, a filesystem
root, or a symlink aliasing any of those. The only accepted destinations are
subdirectories of the Cargo target directory or the checked-in `target/`
directory. Any other root must be passed explicitly with `--allow-dir`, which
is an opt-in to destructive cleanup.

Symlinks are resolved before comparison, so an alias of a rejected path is
rejected too. Paths are compared as resolved absolute paths.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


class GuardError(Exception):
    pass


def _resolved(path: Path) -> Path:
    return Path(path).expanduser().resolve()


def _contains(parent: Path, child: Path) -> bool:
    """True when child is strictly inside parent."""
    try:
        child.relative_to(parent)
    except ValueError:
        return False
    return child != parent


def allowed_roots(repo_root: Path, cargo_target_dir: Path, extra_allowed: list[Path]) -> list[Path]:
    roots = [_resolved(cargo_target_dir), _resolved(repo_root / "target")]
    roots.extend(_resolved(path) for path in extra_allowed)
    return roots


def validate_site_dir(
    repo_root: Path,
    site_dir: Path,
    cargo_target_dir: Path,
    doc_dir: Path,
    extra_allowed: list[Path] | None = None,
) -> Path:
    repo = _resolved(repo_root)
    site = _resolved(site_dir)
    doc = _resolved(doc_dir)

    if site == Path(site.anchor):
        raise GuardError(f"refusing the filesystem root: {site}")
    if site == repo:
        raise GuardError(f"refusing the repository root: {site}")
    if _contains(site, repo):
        raise GuardError(f"refusing an ancestor of the repository: {site} contains {repo}")
    if site == doc or _contains(site, doc):
        raise GuardError(f"refusing a path that contains the rustdoc input: {site} contains {doc}")
    if _contains(doc, site):
        raise GuardError(f"refusing a path inside the rustdoc input: {site}")

    roots = allowed_roots(repo, cargo_target_dir, extra_allowed or [])
    if any(site == root for root in roots):
        raise GuardError(f"refusing an allowed output root itself: {site}")
    if not any(_contains(root, site) for root in roots):
        rendered = ", ".join(str(root) for root in roots)
        raise GuardError(
            f"refusing a destination outside the allowed output roots: {site} "
            f"(allowed: {rendered})"
        )
    return site


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--site-dir", type=Path, required=True)
    parser.add_argument("--cargo-target-dir", type=Path, required=True)
    parser.add_argument("--doc-dir", type=Path, required=True)
    parser.add_argument("--allow-dir", type=Path, action="append", default=[])
    args = parser.parse_args()
    try:
        resolved = validate_site_dir(
            args.repo_root,
            args.site_dir,
            args.cargo_target_dir,
            args.doc_dir,
            args.allow_dir,
        )
    except GuardError as error:
        print(f"site path rejected: {error}", file=sys.stderr)
        return 1
    print(resolved)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
