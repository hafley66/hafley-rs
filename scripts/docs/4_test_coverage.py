#!/usr/bin/env python3
"""Deterministic tests for cargo documented-target classification.

Pure computation on synthetic package metadata, no cargo and no build. Pins the
default-feature rules the API index and checker rely on:

- a lib/bin target with `doc != false` and no unmet `required-features` is
  expected to emit a page;
- `doc = false` is excluded with that reason;
- an unmet `required-features` entry is excluded with that reason;
- a required feature satisfied through the default-feature closure is kept;
- examples/tests/build scripts are excluded as non-lib/bin;
- a hyphenated name maps to an underscored output directory.

    python3 scripts/docs/4_test_coverage.py
"""

from __future__ import annotations

import sys

import cargo_targets as ct


def target(name, kind, doc=True, required=None):
    entry = {"name": name, "kind": [kind], "doc": doc}
    if required is not None:
        entry["required-features"] = required
    return entry


def package(targets, features=None):
    return {"name": "demo", "targets": targets, "features": features or {}}


def reasons(excluded):
    return [reason for _, reason in excluded]


def main() -> int:
    # lib + bin sharing a name: both documented, one emitted directory.
    pkg = package([target("demo", "lib"), target("demo", "bin")])
    documented, excluded = ct.classify_targets(pkg)
    assert len(documented) == 2, documented
    assert not excluded, excluded
    assert {ct.emitted_dir_name(t) for t in documented} == {"demo"}
    print("ok   lib+bin same name share demo/")

    # doc = false.
    documented, excluded = ct.classify_targets(package([target("demo", "lib", doc=False)]))
    assert documented == [] and reasons(excluded) == ["doc = false"], (documented, excluded)
    print("ok   doc = false excluded with reason")

    # required-features unmet under default features.
    pkg = package([target("demo", "lib", required=["extra"])], {"default": []})
    documented, excluded = ct.classify_targets(pkg)
    assert documented == [] and reasons(excluded) == ["requires features extra"], (documented, excluded)
    print("ok   unmet required-features excluded with reason")

    # required-features satisfied directly.
    pkg = package([target("demo", "lib", required=["extra"])], {"default": ["extra"]})
    documented, _ = ct.classify_targets(pkg)
    assert len(documented) == 1, documented
    print("ok   required-features met directly is documented")

    # required-features satisfied through the default closure.
    pkg = package(
        [target("demo", "lib", required=["transitive"])],
        {
            "default": ["agent-read"],
            "agent-read": ["transitive", "dep:serde"],
            "transitive": [],
        },
    )
    documented, _ = ct.classify_targets(pkg)
    assert len(documented) == 1, documented
    print("ok   required-features met through closure is documented")

    # example is not a documented kind.
    documented, excluded = ct.classify_targets(package([target("demo", "example")]))
    assert documented == [] and reasons(excluded) == ["not a lib/bin target"]
    print("ok   example excluded as non-lib/bin")

    # hyphen becomes underscore in the emitted directory.
    assert ct.emitted_dir_name({"name": "boop-acp"}) == "boop_acp"
    print("ok   hyphen maps to underscore directory")

    print("all coverage cases passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
