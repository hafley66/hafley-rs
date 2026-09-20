"""One CaseAnswer per bakeoff case, built from aider's tags (tags.py).

usage: cases.py <case-id>   writes out/aider/<case-id>.json and
                            out/aider/tags/<case-id>.tsv (every tag seen)
"""
import json
import sys
from pathlib import Path

from tags import Ent, aider_raw, enclosing_def, scan

OUT = Path(__file__).resolve().parents[2] / "out" / "aider"
FX = "tests/fixtures/"
FUNC = ("function", "method")


def fn_defs(ents):
    return [e for e in ents if e.kind == "def" and e.tag.split(".")[1] in FUNC]


def def_span(d: Ent):
    """ryi's def span, from the declaration node aider captured. rust spans start
    at the name; go and kotlin spans start at the node (`func` / `fun`)."""
    start = d.name_span[0] if d.path.endswith(".rs") else d.span[0]
    return (start, d.span[1])


def call_pairs(ents):
    """`<caller>-><callee>@<path>:<span>` for every call ref inside a fn body,
    paired with EVERY fn def of the same name (aider joins on the bare name)."""
    defs = fn_defs(ents)
    out = set()
    for r in ents:
        if r.kind != "ref" or r.tag != "reference.call":
            continue
        caller = enclosing_def(ents, r)
        if caller is None:
            continue
        for d in defs:
            if d.name == r.name:
                s = def_span(d)
                out.add(f"{caller.name}->{d.name}@{d.path}:{s[0]}-{s[1]}")
    return sorted(out)


def site(ents, caller_fn, callee):
    """Def sites of every fn named `callee` that the call ref inside
    `caller_fn` could join to. Empty when aider tagged no such ref."""
    refs = [
        r
        for r in ents
        if r.kind == "ref"
        and r.tag == "reference.call"
        and r.name == callee
        and (c := enclosing_def(ents, r)) is not None
        and c.name == caller_fn
    ]
    if not refs:
        return []
    return sorted(
        f"{d.path}:{def_span(d)[0]}-{def_span(d)[1]}" for d in fn_defs(ents) if d.name == callee
    )


def name_join(ents, name):
    """Every tag (def or ref) named `name`, at its name node."""
    return sorted(f"{e.path}:{e.name_span[0]}-{e.name_span[1]}" for e in ents if e.name == name)


CG = FX + "rust_call_grind/"
MC = FX + "rust_macro_callers/"
RN = FX + "rust_rename/fnuse/before/src/"
SR = FX + "rust_spelled_receiver/src/proj.rs"
GO = FX + "go_field_promote/"
KT = FX + "kotlin_receivers/"
DP = FX + "deps/"

FILES = {
    "chain-receiver-call": [CG + "widget.rs", CG + "decoy.rs"],
    "deps-reach-app": [DP + f for f in ("app.ts", "lib/bare.ts", "lib/helper.ts", "lib/mapped.ts", "lib/util.ts", "side.ts", "widget/index.ts")],
    "go-field-promotion": [GO + "caller/caller.go", GO + "lib/lib.go", GO + "base/base.go"],
    "kotlin-ambiguous-receiver": [KT + "use.kt", KT + "lib.kt"],
    "kotlin-shadow-decline": [KT + "use.kt", KT + "lib.kt"],
    "macro-cross-file-miss": [MC + f for f in ("user.rs", "decoys.rs", "macros.rs", "local.rs")],
    "rename-safe-occurrence-set": [RN + "lib.rs", RN + "util.rs"],
    "spelled-receiver-field-chain": [SR],
    "spelled-receiver-trait-bound": [SR],
    "variant-literal-not-call": [CG + "shapes.rs"],
}


def answer(case, ents):
    """(entries, notes) for one case."""
    if case == "chain-receiver-call":
        return call_pairs(ents), (
            "name-join: aider pairs each call ref with every fn def of that name, so "
            "decoy.rs tick/read ride along; Widget::new() is a scoped_identifier callee "
            "the rust query never tags, so no ->new pair"
        )
    if case == "go-field-promotion":
        return site(ents, "UseOuter", "Ring"), (
            "name-join: ref Ring in UseOuter joins both Ring defs; aider has no field types"
        )
    if case == "kotlin-ambiguous-receiver":
        return site(ents, "fieldLeg", "run"), (
            "name-join: ref run in fieldLeg joins Widget.run, Decoy.run and Inner.run"
        )
    if case == "spelled-receiver-field-chain":
        return site(ents, "field_leg", "run"), (
            "name-join: only Widget::run is tagged (trait method signatures are not "
            "function_item), so the join has one candidate"
        )
    if case == "spelled-receiver-trait-bound":
        return site(ents, "trait_bound_leg", "run"), (
            "name-join: only Widget::run is tagged; the Proj::run signature has no def tag, "
            "so the join lands on the same-named decoy"
        )
    if case == "rename-safe-occurrence-set":
        return name_join(ents, "Helper"), (
            "name-join: every tag named Helper; aider has no scopes, so lib.rs's own struct "
            "and the impl ref ride along and fn a's Helper::new() (scoped callee) is untagged"
        )
    if case == "variant-literal-not-call":
        return call_pairs(ents), (
            "no reference.call tag exists in shapes.rs: a struct-variant literal is a "
            "struct_expression, which the rust query does not tag"
        )
    if case == "macro-cross-file-miss":
        pairs = call_pairs(ents)
        minted = [d.name for d in fn_defs(ents) if d.name in ("alpha", "beta", "gamma")]
        assert not pairs and not minted, (pairs, minted)
        return [], (
            "cannot: macro_rules bodies are token trees, so the rust query tags no fn def "
            "or call inside them; aider sees mint_helpers only as a macro def and an invocation"
        )
    if case == "kotlin-shadow-decline":
        return [], (
            "cannot: aider has no unresolved or decline concept; it tags the ref run in "
            "shadow and joins it to the run defs by name"
        )
    if case == "deps-reach-app":
        imports = [e for e in ents if "import" in e.tag or "module" in e.tag]
        assert not imports, imports
        return [], (
            "cannot: the typescript query tags no import, export or specifier, and aider "
            "has no module resolver, tsconfig paths, or unresolved reasons"
        )
    raise SystemExit(f"unknown case {case}")


def main(case):
    files = FILES[case]
    ents = [e for f in files for e in scan(f)]
    # aider's own Tag list must agree with the span-carrying rescan.
    for f in files:
        raw = {(n, k, ln) for n, k, ln in aider_raw(f) if ln > 0}
        mine = {(e.name, e.kind, e.line) for e in ents if e.path == f}
        assert raw == mine, (f, raw ^ mine)
    entries, notes = answer(case, ents)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "tags").mkdir(exist_ok=True)
    with open(OUT / "tags" / f"{case}.tsv", "w") as fh:
        for e in sorted(ents, key=lambda e: (e.path, e.span, e.kind, e.name)):
            fh.write(
                f"{e.path}\t{e.kind}\t{e.tag}\t{e.name}\tL{e.line}\t{e.span[0]}-{e.span[1]}\t"
                f"name={e.name_span[0]}-{e.name_span[1]}\n"
            )
    doc = {"case": case, "answer": entries, "notes": notes}
    (OUT / f"{case}.json").write_text(json.dumps(doc, indent=2) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
