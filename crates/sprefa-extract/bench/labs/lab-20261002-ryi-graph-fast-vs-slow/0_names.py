"""Pick graph anchors per language from the fast stores: collisions, trait/overload, generics, re-exports, JSX, singles.

usage: python3 0_names.py  -> names.tsv (lang, category, arm, anchor)
"""
import os, re, sqlite3, pathlib

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent.parent
DB = BENCH / "repos" / "stress-db"
WT = BENCH.parent.parent.parent  # worktree root
CORPORA = {
    "rust": (WT, WT / "crates" / "hafley_scm", DB / "scm-fast.db", DB / "scm-types.db", (".rs",)),
    "ts": (BENCH / "repos" / "hafley-rxjs-main", BENCH / "repos" / "hafley-rxjs-main" / "packages",
           DB / "rx-fast.db", DB / "rx-types.db", (".ts", ".tsx")),
}


def sources(base, exts):
    for path in base.rglob("*"):
        if path.suffix in exts and path.is_file() and "node_modules" not in path.parts:
            yield path, path.read_text(errors="replace")


def pick(lang):
    root, base, calls_db, types_db, exts = CORPORA[lang]
    calls = sqlite3.connect(calls_db)
    callee = dict(calls.execute(
        "select callee_name, count(*) from resolved_edge where callee_name is not null group by 1"))
    defs = {name: n for name, n in calls.execute(
        "select callee_name, count(distinct callee_path||':'||callee_start) from resolved_edge group by 1")}
    first_def = dict(calls.execute(
        "select callee_name, min(callee_path) from resolved_edge group by 1"))
    files = list(sources(base, exts))
    text = "\n".join(body for _, body in files)
    out = []

    def add(category, names, k):
        taken = 0
        for name in names:
            if taken == k:
                break
            if name in callee and not any(name == row[3] for row in out):
                out.append((lang, category, "call", name))
                taken += 1

    by_fan_in = sorted(callee, key=lambda name: -callee[name])
    add("collision", [n for n in by_fan_in if defs.get(n, 0) > 1], 6)
    if lang == "rust":
        trait_fns = set(re.findall(r"\btrait \w+[^{]*\{([^}]*)\}", text))
        trait_names = set()
        for body in trait_fns:
            trait_names.update(re.findall(r"\bfn (\w+)", body))
        add("trait_method", [n for n in by_fan_in if n in trait_names], 4)
        generics = set(re.findall(r"\bfn (\w+)<", text))
        add("generic", [n for n in by_fan_in if n in generics], 4)
        reexported = set(re.findall(r"pub use [^;]*?\b(\w+)\s*;", text))
        add("reexport", [n for n in by_fan_in if n in reexported], 3)
    else:
        from collections import Counter
        declared = Counter(re.findall(r"(?m)^\s*export (?:declare )?function (\w+)", text))
        overloads = {n for n, k in declared.items() if k > 1}
        add("overload", [n for n in by_fan_in if n in overloads], 3)
        default = set(re.findall(r"export default (?:async )?function (\w+)", text))
        add("default_export", [n for n in by_fan_in if n in default], 2)
        barrel = set()
        for path, body in files:
            if path.stem == "index":
                for group in re.findall(r"export \{([^}]*)\} from", body):
                    barrel.update(re.findall(r"\b(\w+)\b(?!\s+as)", group))
        add("barrel", [n for n in by_fan_in if n in barrel], 3)
        jsx = set(re.findall(r"<([A-Z]\w*)[\s/>]", text))
        add("jsx_component", [n for n in by_fan_in if n in jsx], 3)
        generics = set(re.findall(r"function (\w+)<", text))
        add("generic", [n for n in by_fan_in if n in generics], 3)
    add("single", [n for n in by_fan_in if defs.get(n) == 1], 5)
    # PATH#NAME for every collision: the path of one definition.
    for lang_, category, arm, name in list(out):
        if category == "collision":
            out.append((lang, "collision_path", "call", f"{first_def[name]}#{name}"))
    types = sqlite3.connect(types_db)
    type_rows = types.execute(
        "select target_name, count(*), count(distinct target_path) from resolved_type_edge "
        "where target_name is not null group by 1 order by 2 desc").fetchall()
    collide = [name for name, _, paths in type_rows if paths > 1][:3]
    single = [name for name, _, paths in type_rows if paths == 1][:3]
    for name in collide:
        out.append((lang, "type_collision", "type", name))
    for name in single:
        out.append((lang, "type_single", "type", name))
    return out


if __name__ == "__main__":
    rows = pick("rust") + pick("ts")
    with open(HERE / "names.tsv", "w") as handle:
        handle.write("lang\tcategory\tplane\tanchor\n")
        for row in rows:
            handle.write("\t".join(row) + "\n")
    print(len(rows), "anchors")
