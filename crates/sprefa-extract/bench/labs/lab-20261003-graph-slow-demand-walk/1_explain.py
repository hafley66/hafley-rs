"""Each eager-only / walk-only node of results.db against an eager store published with `--sqlite eager-store.db`.

eager-only: the eager first-discovery chain to it, and which chain node is a (path, name) shared by several fns.
walk-only: whether any eager resolved_edge targets it at all.
usage: python3 1_explain.py
"""
import collections, json, pathlib, re, sqlite3

HERE = pathlib.Path(__file__).resolve().parent
WT = HERE.parent.parent.parent.parent.parent
results = sqlite3.connect(HERE / "results.db")
store = sqlite3.connect(HERE / "eager-store.db")
edges = store.execute("select caller_path, caller_name, callee_path, callee_name from resolved_edge "
                      "where callee_path is not null and callee_path != ''").fetchall()
out = collections.defaultdict(set)
targeted = set()
for a, b, c, d in edges:
    out[(a, b)].add((c, d))
    targeted.add((c, d))


def fns_named(path, name):
    try:
        return len(re.findall(rf"\bfn {re.escape(name)}\b", (WT / path).read_text()))
    except OSError:
        return 0


def chain(anchor, node):
    path, name = anchor.split("#") if "#" in anchor else (None, anchor)
    starts = [n for n in out if n[1] == name and (path is None or n[0].endswith(path))]
    parent = {s: None for s in starts}
    queue = collections.deque(starts)
    while queue:
        current = queue.popleft()
        for nxt in sorted(out[current]):
            if nxt not in parent:
                parent[nxt] = current
                queue.append(nxt)
    if node not in parent:
        return None
    path_nodes = []
    while node is not None:
        path_nodes.append(node)
        node = parent[node]
    return list(reversed(path_nodes))


for anchor, tier, key in results.execute(
        "select anchor, tier, key from edge e where not exists (select 1 from edge o where o.anchor = e.anchor "
        "and o.tier <> e.tier and o.key = e.key) order by tier, anchor, key"):
    node = tuple(json.loads(key))
    if tier == "eager":
        steps = chain(anchor, node)
        shared = [f"{p}#{n}x{fns_named(p, n)}" for p, n in (steps or [])[:-1] if fns_named(p, n) > 1]
        print("eager_only", anchor, "#".join(node), "collapse_at=" + ",".join(shared) if shared else "no_collapse",
              " > ".join(n for _, n in steps or []), sep="\t")
    else:
        print("walk_only", anchor, "#".join(node), "eager_targets_it" if node in targeted else "eager_never_targets",
              f"fn_decls={fns_named(*node)}", sep="\t")
