"""Structure metrics for a rust tree, off `extract --family cst` node records.

usage: structure.py <extract-binary> <tree-root> <label>
prints one JSON object with per-tree totals and per-file rows.
"""
import json
import os
import subprocess
import sys
from collections import Counter, defaultdict

EXTRACT, ROOT, LABEL = sys.argv[1], sys.argv[2], sys.argv[3]
KINDS = [
    "function_item", "struct_item", "enum_item", "trait_item", "impl_item",
    "match_expression", "match_arm", "type_parameters", "closure_expression",
    "if_expression", "let_condition", "macro_invocation", "unsafe_block",
    "where_clause", "attribute_item", "enum_variant", "field_declaration",
    "type_item", "const_item", "static_item", "mod_item", "use_declaration",
]


def files():
    for dirpath, _, names in os.walk(ROOT):
        for name in names:
            if name.endswith(".rs"):
                yield os.path.join(dirpath, name)


def nodes(path):
    out = subprocess.run([EXTRACT, "--family", "cst", path], capture_output=True, text=True)
    for line in out.stdout.splitlines():
        row = json.loads(line)
        if row.get("record") == "node":
            yield row


def line_of(text, offset):
    return text.count("\n", 0, offset)


rows = []
totals = Counter()
fn_lines_all = []
arms_per_match_all = []
impl_self_types = Counter()
for path in sorted(files()):
    rel = os.path.relpath(path, ROOT)
    text = open(path, encoding="utf-8", errors="replace").read()
    counts = Counter()
    fns = []
    matches = []
    arms = []
    impls = []
    depth_max = 0
    for row in nodes(path):
        kind = row["kind"]
        s, e = row["span"]["start"], row["span"]["end"]
        if kind in KINDS:
            counts[kind] += 1
        if kind == "function_item":
            fns.append((s, e, row.get("name")))
        elif kind == "match_expression":
            matches.append((s, e))
        elif kind == "match_arm":
            arms.append((s, e))
        elif kind == "impl_item":
            impls.append((s, e))
        elif kind == "block":
            # nesting depth: blocks containing this block
            pass
    fn_lines = [line_of(text, e) - line_of(text, s) + 1 for s, e, _ in fns]
    fn_lines_all.extend(fn_lines)
    arms_per_match = []
    for ms, me in matches:
        arms_per_match.append(sum(1 for a, b in arms if ms <= a and b <= me and not any(ms < m2s <= a and b <= m2e < me for m2s, m2e in matches)))
    arms_per_match_all.extend(arms_per_match)
    # impl blocks per self type: `impl X {` / `impl T for X {`
    for s, e in impls:
        head = text[s:text.find("{", s)] if text.find("{", s) > 0 else text[s:s+80]
        head = head.replace("\n", " ")
        self_ty = head.split(" for ")[-1].strip() if " for " in head else head.replace("impl", "", 1).strip()
        self_ty = self_ty.split("<")[0].split(" ")[0].strip()
        impl_self_types[(rel, self_ty)] += 1
    long_fns = sorted([(n, name or "?") for (s, e, name), n in zip(fns, fn_lines) if n > 60], reverse=True)
    lines = text.count("\n") + 1
    rows.append({
        "file": rel,
        "lines": lines,
        "fns": len(fns),
        "fn_lines_p50": sorted(fn_lines)[len(fn_lines) // 2] if fn_lines else 0,
        "fn_lines_max": max(fn_lines) if fn_lines else 0,
        "fns_over_60": len(long_fns),
        "long_fns": long_fns[:6],
        "match": counts["match_expression"],
        "arms": counts["match_arm"],
        "arms_max": max(arms_per_match) if arms_per_match else 0,
        "struct": counts["struct_item"],
        "enum": counts["enum_item"],
        "variants": counts["enum_variant"],
        "trait": counts["trait_item"],
        "impl": counts["impl_item"],
        "generics": counts["type_parameters"],
        "closures": counts["closure_expression"],
        "if": counts["if_expression"],
        "macros": counts["macro_invocation"],
        "unsafe": counts["unsafe_block"],
    })
    totals.update(counts)
    totals["lines"] += lines
    totals["files"] += 1

fn_lines_all.sort()
summary = {
    "label": LABEL,
    "files": totals["files"],
    "lines": totals["lines"],
    "fns": totals["function_item"],
    "fn_lines_p50": fn_lines_all[len(fn_lines_all) // 2] if fn_lines_all else 0,
    "fn_lines_p90": fn_lines_all[int(len(fn_lines_all) * 0.9)] if fn_lines_all else 0,
    "fn_lines_max": fn_lines_all[-1] if fn_lines_all else 0,
    "fns_over_60": sum(1 for n in fn_lines_all if n > 60),
    "fns_over_120": sum(1 for n in fn_lines_all if n > 120),
    "match": totals["match_expression"],
    "arms": totals["match_arm"],
    "arms_per_match_p90": sorted(arms_per_match_all)[int(len(arms_per_match_all) * 0.9)] if arms_per_match_all else 0,
    "struct": totals["struct_item"],
    "enum": totals["enum_item"],
    "variants": totals["enum_variant"],
    "trait": totals["trait_item"],
    "impl": totals["impl_item"],
    "generics": totals["type_parameters"],
    "closures": totals["closure_expression"],
    "if": totals["if_expression"],
    "macros": totals["macro_invocation"],
    "unsafe": totals["unsafe_block"],
    "impl_blocks_same_type_gt1": sorted([(f, t, n) for (f, t), n in impl_self_types.items() if n > 1], key=lambda x: -x[2])[:15],
}
print(json.dumps({"summary": summary, "files": rows}))
