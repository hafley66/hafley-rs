#!/usr/bin/env python3
"""Deterministic tests for the generated-site destination guard.

Pure path logic, no build and no network. Exits nonzero on the first failed
expectation and prints every case. Run from anywhere:

    python3 scripts/docs/3_test_guards.py
"""

from __future__ import annotations

import sys
import tempfile
import traceback
from pathlib import Path

import site_paths
from site_paths import GuardError


def expect_rejected(label: str, fn) -> None:
    try:
        fn()
    except GuardError:
        print(f"ok   reject: {label}")
        return
    raise AssertionError(f"expected rejection: {label}")


def expect_allowed(label: str, fn, expected: Path) -> None:
    result = fn()
    resolved = expected.resolve()
    if result != resolved:
        raise AssertionError(f"{label}: got {result}, expected {resolved}")
    print(f"ok   accept: {label} -> {result}")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="docs-guard-") as raw:
        base = Path(raw).resolve()
        repo = base / "repo"
        (repo / "target").mkdir(parents=True)
        (repo / "crates" / "boop").mkdir(parents=True)
        (repo / "docs" / "book" / "src").mkdir(parents=True)
        external = base / "ext-target"
        external.mkdir()
        doc = repo / "target" / "doc"
        doc.mkdir()

        (base / "alias-repo").symlink_to(repo)
        (base / "alias-target").symlink_to(repo / "target")
        (base / "link-to-site").symlink_to(repo / "target" / "docs-site")
        (base / "other").mkdir()

        def validate(site: Path, target_dir: Path = repo / "target", doc_dir: Path = doc, extra=()):
            return site_paths.validate_site_dir(repo, site, target_dir, doc_dir, list(extra))

        expect_rejected("filesystem root", lambda: validate(Path("/")))
        expect_rejected("repository root", lambda: validate(repo))
        expect_rejected("ancestor of repository", lambda: validate(base))
        expect_rejected("home directory", lambda: validate(Path.home()))
        expect_rejected("source tree", lambda: validate(repo / "crates"))
        expect_rejected("rustdoc input", lambda: validate(doc))
        expect_rejected("inside rustdoc input", lambda: validate(doc / "site"))
        expect_rejected("cargo target root itself", lambda: validate(repo / "target"))
        expect_rejected("symlink to repository", lambda: validate(base / "alias-repo"))
        expect_rejected("symlink to target root", lambda: validate(base / "alias-target"))
        expect_rejected("traversal to ancestor", lambda: validate(repo / "target" / ".." / ".."))
        expect_rejected("outside allowed roots", lambda: validate(base / "other"))
        expect_rejected(
            "allow-dir root itself",
            lambda: validate(base / "other", extra=[base / "other"]),
        )

        expect_allowed("repo target subdir", lambda: validate(repo / "target" / "docs-site"), repo / "target" / "docs-site")
        expect_allowed(
            "nested repo target subdir",
            lambda: validate(repo / "target" / "a" / "b"),
            repo / "target" / "a" / "b",
        )
        expect_allowed(
            "external cargo target subdir",
            lambda: validate(external / "docs-site", target_dir=external, doc_dir=external / "doc"),
            external / "docs-site",
        )
        expect_allowed(
            "symlink resolving into target",
            lambda: validate(base / "link-to-site"),
            repo / "target" / "docs-site",
        )
        expect_allowed(
            "explicit allow-dir subdir",
            lambda: validate(base / "other" / "site", extra=[base / "other"]),
            base / "other" / "site",
        )

    print("all guard cases passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError:
        traceback.print_exc()
        raise SystemExit(1)
