#!/usr/bin/env python3
"""Cargo workspace and target facts shared by the API index and the checker.

The rules encoded here mirror `cargo doc --workspace --no-deps --locked` with
default features:

- only `lib`, `bin`, and `proc-macro` targets emit rustdoc;
- a target with `doc = false` is not documented;
- a target whose `required-features` are not all enabled by the package's
  default feature set is not built, so it is not documented;
- rustdoc names the output directory after the crate name, so a hyphen in the
  package or target name becomes an underscore (`boop-acp` -> `boop_acp`).

A package may document a library and a binary that share one name; both write
the same `target/doc/<name>/` directory, so callers must key pages by emitted
directory name rather than by target.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

DOCUMENTED_KINDS = ("lib", "bin", "proc-macro")


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
        (package for package in metadata["packages"] if package["id"] in members),
        key=lambda package: package["name"],
    )


def emitted_dir_name(target: dict) -> str:
    return target["name"].replace("-", "_")


def default_feature_closure(package: dict) -> set[str]:
    features = package.get("features", {})
    seen: set[str] = set()
    stack = list(features.get("default", []))
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        seen.add(name)
        for dep in features.get(name, []):
            if dep.startswith("dep:"):
                continue
            dep_name = dep.split("/", 1)[0]
            if dep_name in features:
                stack.append(dep_name)
    return seen


def classify_targets(package: dict) -> tuple[list[dict], list[tuple[dict, str]]]:
    """Split a package's targets into default-documented and excluded ones.

    Returns (documented, excluded), where excluded is a list of
    (target, reason). Nothing is dropped: every target lands in one list.
    """
    enabled = default_feature_closure(package)
    documented: list[dict] = []
    excluded: list[tuple[dict, str]] = []
    for target in package["targets"]:
        if not any(kind in target["kind"] for kind in DOCUMENTED_KINDS):
            excluded.append((target, "not a lib/bin target"))
            continue
        if not target.get("doc", True):
            excluded.append((target, "doc = false"))
            continue
        required = set(target.get("required-features", []))
        missing = sorted(required - enabled)
        if missing:
            excluded.append((target, "requires features " + ", ".join(missing)))
            continue
        documented.append(target)
    return documented, excluded


def target_kind_label(target: dict) -> str:
    labels = {"lib": "library", "bin": "binary", "proc-macro": "proc-macro"}
    return "+".join(labels[k] for k in target["kind"] if k in labels)
