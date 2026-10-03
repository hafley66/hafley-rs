"""Walk-only nodes of results.db by the walk edge kind that first reached them.

Needs walk-edges-ANCHOR.log per anchor from
  RUST_LOG=rust_walk.edge=debug,warn ryii graph --slow --from ANCHOR ... 2> walk-edges-ANCHOR.log
usage: python3 2_kinds.py ANCHOR...
"""
import collections, json, pathlib, re, sqlite3, sys

HERE = pathlib.Path(__file__).resolve().parent
WT = HERE.parent.parent.parent.parent.parent
db = sqlite3.connect(HERE / "results.db")
causes = collections.Counter()
for anchor in sys.argv[1:]:
    first = {}
    log = HERE / f"walk-edges-{anchor.replace('/', '_')}.log"
    for line in log.read_text().splitlines() if log.exists() else []:
        fields = dict(re.findall(r'(\w+)=("[^"]*"|\S+)', line))
        if "to_path" in fields:
            first.setdefault((fields["to_path"], fields["to_name"]), fields["kind"])
    for (key,) in db.execute("select key from edge e where anchor = ? and tier = 'walk' and not exists "
                             "(select 1 from edge o where o.anchor = e.anchor and o.tier = 'eager' and o.key = e.key)",
                             (anchor,)):
        path, name = json.loads(key)
        kind = first.get((path, name), "Call")
        declared = re.search(rf"\bfn {re.escape(name)}\b", (WT / path).read_text())
        cause = kind if kind in ("Passed", "TraitImpl") else "variant" if name[0].isupper() else \
            "derive" if not declared else "downstream"
        causes[cause] += 1
        print(anchor, f"{path}#{name}", kind, cause, sep="\t")
print(dict(causes), sum(causes.values()))
