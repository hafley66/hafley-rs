"""T1-T4 oracles in Python over db/lab-<corpus>.db; tree walks reuse the stress lab's Tree.

usage: python3 1_oracle.py small|crates  -> db/oracle/<task>_<corpus>[_<tier>].tsv; fills t4_pair in lab-<corpus>.db and .duckdb
"""
import collections, importlib, pathlib, sqlite3, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(HERE.parent / "lab-20261002-ryi-scmpp-stress"))
Tree = importlib.import_module("2_oracle").Tree
SPINE = {"function", "value"}
TIERS = ("fast", "slow")


def write(name, header, rows):
    out = HERE / "db" / "oracle"
    out.mkdir(parents=True, exist_ok=True)
    lines = ["\t".join(header)] + ["\t".join(map(str, row)) for row in sorted(set(rows))]
    (out / f"{name}.tsv").write_text("\n".join(lines) + "\n")
    print(name, len(lines) - 1)


def cst(db, corpus):
    chain, self_call = [], []
    for file, path in db.execute("select id, text from scmpp_dict_path order by id").fetchall():
        tree = Tree(db, file)
        source = (REPO / path).read_bytes()
        nodes = set(tree.parent) | set(tree.children)
        kind = lambda node: node[2]
        pairs = []
        for origin in (node for node in nodes if kind(node) == "call_expression"):
            frontier = [origin]
            while frontier:
                node = frontier.pop()
                for _, _, child in tree.children.get(node, []):
                    if tree.field.get(child) in SPINE:
                        frontier.append(child)
                        if kind(child) == "call_expression":
                            pairs.append((origin, child))
        links = {link for _, link in pairs}
        chain += [(path, o[0], o[1], l[0], l[1]) for o, l in pairs if o not in links]
        for fn in (node for node in nodes if kind(node) == "function_item"):
            field = {tree.field.get(child): child for _, _, child in tree.children.get(fn, [])}
            name, body = field.get("name"), field.get("body")
            if name is None or body is None:
                continue
            text = source[name[0]:name[1]]
            for call in tree.descendants(body, lambda node: False):
                if kind(call) != "call_expression":
                    continue
                callee = next((c for _, _, c in tree.children.get(call, []) if tree.field.get(c) == "function"), None)
                if callee and kind(callee) == "identifier" and source[callee[0]:callee[1]] == text:
                    self_call.append((path, fn[0], fn[1]))
                    break
    write(f"T1_{corpus}", ("path", "outer_start", "outer_end", "link_start", "link_end"), chain)
    write(f"T2_{corpus}", ("path", "fn_start", "fn_end"), self_call)


def graph(db, corpus):
    text = dict((i, (p, n)) for i, p, n in db.execute(
        "select f.id, p.text, n.text from fn f join fn_dict_path p on p.id = f.path_id join fn_dict_name n on n.id = f.name_id"))
    seeds = db.execute("select anchor, fn_id from seed order by anchor").fetchall()
    db.execute("delete from t4_pair")
    for tier in TIERS:
        out = collections.defaultdict(list)
        for src, dst in db.execute(f"select src_fn_id, dst_fn_id from call_edge_{tier} where extern = 0"):
            out[src].append(dst)
        reach, shortest = [], []
        for anchor, seed in seeds:
            depth, frontier, level = {}, [seed], 0
            while frontier:
                level += 1
                frontier = [d for s in frontier for d in out[s]]
                frontier = list(dict.fromkeys(d for d in frontier if d not in depth))
                for d in frontier:
                    depth[d] = level
            reach += [(anchor, *text[fn]) for fn in depth]
            if depth:
                far = max(depth.values())
                target = min((fn for fn in depth if depth[fn] == far), key=lambda fn: text[fn])
                db.execute("insert into t4_pair values (?,?,?,?)", (tier, anchor, seed, target))
                shortest.append((anchor, *text[target], far))
        write(f"T3_{corpus}_{tier}", ("anchor", "path", "name"), reach)
        write(f"T4_{corpus}_{tier}", ("anchor", "target_path", "target_name", "length"), shortest)
    db.commit()


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    corpus = sys.argv[1]
    lab = HERE / "db" / f"lab-{corpus}.db"
    db = sqlite3.connect(lab)
    graph(db, corpus)
    cst(db, corpus)
    duck = HERE / "db" / f"lab-{corpus}.duckdb"
    subprocess.run([str(HERE / "db" / "bin" / "duckdb"), str(duck), "-c",
                    f"LOAD sqlite; DROP TABLE IF EXISTS t4_pair; CREATE TABLE t4_pair AS SELECT * FROM sqlite_scan('{lab}', 't4_pair');"
                    f"COPY t4_pair TO '{HERE / 'db' / f'parquet-{corpus}' / 't4_pair.parquet'}' (FORMAT parquet);"], check=True)
