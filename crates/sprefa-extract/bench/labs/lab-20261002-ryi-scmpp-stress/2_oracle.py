"""Independent oracle for single-relation scm++ queries: tree walks in Python over the run's own CST edge rows.

The oracle trusts tree-sitter for level matches (capture rows of pattern 0) and node kinds (edge rows),
and recomputes every relation from parent/child/sibling structure; scm++ answers from its lowered SQL.

usage: python3 2_oracle.py CORPUS_DIR   -> oracle.tsv (case, relation, options, scmpp_rows, oracle_rows, agree, scmpp_only, oracle_only)
"""
import os, sqlite3, subprocess, sys, pathlib, time, collections

HERE = pathlib.Path(__file__).resolve().parent
RYII = HERE.parent.parent.parent / "target" / "release" / "ryii"
OUT = HERE / "oracle-db"

# (case, level-0 kind, relation, target kind, options dict)
CASES = []
PAIRS = [
    ("call_expression", "closure_expression"),
    ("identifier", "function_item"),
    ("let_declaration", "expression_statement"),
    ("block", "return_expression"),
    ("match_arm", "macro_invocation"),
    ("field_expression", "call_expression"),
]
for here, there in PAIRS:
    for relation in ("has", "has-ancestor", "has-parent", "precedes", "follows"):
        for option in ("", "neighbor", "stop", "field", "not"):
            if relation == "has-parent" and option in ("neighbor", "stop"):
                continue
            CASES.append((f"{here}/{relation}/{there}/{option or 'default'}", here, relation, there, option))
    for n in (1, 2, 3):
        CASES.append((f"{here}/nth-child/{n}", here, "nth-child", None, str(n)))
        CASES.append((f"{here}/nth-child/{n}/of", here, "nth-child-of", here, str(n)))

STOP = "block"
FIELD = {"has": "body", "has-ancestor": "body", "has-parent": "body", "precedes": None, "follows": None}


def query(here, relation, there, option):
    if relation == "nth-child":
        return f"(({here}) @x (#nth-child? @x {option}))"
    if relation == "nth-child-of":
        return f"(({here}) @x (#nth-child? @x {option} of {there}))"
    op = ("not-" if option == "not" else "") + relation
    extra = {"neighbor": " stopBy: neighbor", "stop": f" stopBy: ({STOP})", "field": f" field: {FIELD[relation] or 'body'}"}.get(option, "")
    if option == "field" and FIELD[relation] is None:
        extra = " field: arguments"
    return f"(({here}) @x (#{op}? @x {there}{extra}))"


class Tree:
    def __init__(self, db, cid):
        self.parent, self.children, self.field = {}, collections.defaultdict(list), {}
        for fs, fe, fk, ts, te, tk, field, index, named in db.execute(
                "select from__start, from__end, from_kind, to__start, to__end, to_kind, field, \"index\", named_index "
                "from edge where family='cst' and _content_id=?", (cid,)):
            parent, child = (fs, fe, fk), (ts, te, tk)
            self.parent[child] = parent
            self.children[parent].append((index, named, child))
            self.field[child] = field
        for kids in self.children.values():
            kids.sort()

    def named_siblings(self, node):
        parent = self.parent.get(node)
        if parent is None:
            return None, []
        kids = [child for _, named, child in self.children[parent] if named is not None]
        return parent, kids

    def descendants(self, node, stop):
        for _, _, child in self.children.get(node, []):
            yield child
            if not stop(child):
                yield from self.descendants(child, stop)


def oracle(tree, x, relation, there, option):
    kind = lambda node: node[2]
    target = lambda node: kind(node) == there
    stop = (lambda node: kind(node) == STOP) if option == "stop" else (lambda node: False)
    if relation == "nth-child":
        _, kids = tree.named_siblings(x)
        return x in kids and kids.index(x) + 1 == int(option)
    if relation == "nth-child-of":
        _, kids = tree.named_siblings(x)
        kids = [k for k in kids if kind(k) == there]
        return x in kids and kids.index(x) + 1 == int(option)
    want_field = (FIELD.get(relation) or "arguments") if option == "field" else None
    found = False
    if relation == "has":
        if option == "neighbor":
            pool = [(child, tree.field.get(child)) for _, _, child in tree.children.get(x, [])]
        else:
            pool = [(node, tree.field.get(node)) for node in tree.descendants(x, stop)]
        found = any(target(node) and (want_field is None or field == want_field) for node, field in pool)
    elif relation in ("has-ancestor", "has-parent"):
        node = x
        while node in tree.parent:
            parent = tree.parent[node]
            if target(parent) and (want_field is None or tree.field.get(node) == want_field):
                found = True
                break
            if relation == "has-parent" or option == "neighbor" or stop(parent):
                break
            node = parent
    else:
        _, kids = tree.named_siblings(x)
        if x in kids:
            at = kids.index(x)
            walk = kids[at + 1:] if relation == "precedes" else list(reversed(kids[:at]))
            if option == "neighbor":
                walk = walk[:1]
            for node in walk:
                if target(node) and (want_field is None or tree.field.get(node) == want_field):
                    found = True
                    break
                if stop(node):
                    break
    return (not found) if option == "not" else found


def main(corpus):
    OUT.mkdir(exist_ok=True)
    lines = ["case\trelation\toption\tscmpp_rows\toracle_rows\tagree\tscmpp_only\toracle_only\tseconds"]
    for case, here, relation, there, option in CASES:
        db_path = OUT / (case.replace("/", "__") + ".db")
        db_path.unlink(missing_ok=True)
        scm = OUT / "q.scm"
        scm.write_text(query(here, relation, there, option) + "\n")
        started = time.time()
        done = subprocess.run([str(RYII), "query", "--scmpp", str(scm), "--sqlite", str(db_path), "--pattern", "*.rs", corpus],
                              capture_output=True, text=True, env=dict(os.environ, RUST_LOG="warn"))
        seconds = time.time() - started
        if done.returncode != 0:
            lines.append(f"{case}\t{relation}\t{option}\terror\t\t\t\t\t{seconds:.2f}")
            print(case, "ERROR", done.stderr[-300:])
            continue
        db = sqlite3.connect(db_path)
        got = set(db.execute("select path, x__start, x__end from scmpp_row"))
        trees = {}
        want = set()
        for path, cid, start, end, kind in db.execute(
                "select c._input_path, c._content_id, c.start, c.\"end\", c.kind from capture c "
                "where c.pattern = 0 and c.capture = 'x'"):
            if cid not in trees:
                trees[cid] = Tree(db, cid)
            if oracle(trees[cid], (start, end, kind), relation, there, option):
                want.add((path, start, end))
        agree = len(got & want)
        lines.append(f"{case}\t{relation}\t{option}\t{len(got)}\t{len(want)}\t{agree}\t{len(got - want)}\t{len(want - got)}\t{seconds:.2f}")
        if got != want:
            print(case, "DISAGREE", sorted(got - want)[:3], sorted(want - got)[:3])
        db.close()
        db_path.unlink()
    (HERE / "oracle.tsv").write_text("\n".join(lines) + "\n")
    print(len(CASES), "cases")


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    main(sys.argv[1])
