"""Every def/ref tag aider's repo map extracts, with the byte span of the
node its `.scm` captured. aider's own Tag carries only (name, kind, line); the
span comes from re-running aider's query on the same tree (see NOTES.md,
"Span conversion")."""
import sys
import tempfile
from collections import defaultdict, namedtuple
from pathlib import Path

from aider.io import InputOutput
from aider.repomap import RepoMap, get_scm_fname
from grep_ast import filename_to_lang
from grep_ast.tsl import get_language, get_parser
from tree_sitter import Query, QueryCursor

# name_span is the `@name.*` node; span is the enclosing `@definition.*` /
# `@reference.*` node. For a def, span is the whole declaration.
Ent = namedtuple("Ent", "path kind tag name line span name_span")

FIXTURE_ROOT = Path(__file__).resolve().parents[3] / "sprefa-extract"


def scan(rel_path: str):
    """Yield Ent for every def/ref capture aider's query makes in one file."""
    path = FIXTURE_ROOT / rel_path
    lang = filename_to_lang(str(path))
    if not lang:
        return
    scm = get_scm_fname(lang)
    if not scm or not scm.exists():
        return
    data = path.read_bytes()
    tree = get_parser(lang).parse(data)
    caps = QueryCursor(Query(get_language(lang), scm.read_text())).captures(tree.root_node)
    shells = defaultdict(list)  # "definition.method" -> [node]
    names = []
    for tag, nodes in caps.items():
        if tag.startswith("name."):
            names += [(tag, n) for n in nodes]
        elif tag.startswith(("definition.", "reference.")):
            shells[tag] += nodes
    seen = set()
    for tag, n in names:
        if tag.startswith("name.definition."):
            kind, sub = "def", tag[len("name.") :]
        elif tag.startswith("name.reference."):
            kind, sub = "ref", tag[len("name.") :]
        else:
            continue
        # smallest shell of the same subtype that contains the name node
        cand = [s for s in shells[sub] if s.start_byte <= n.start_byte and n.end_byte <= s.end_byte]
        shell = min(cand, key=lambda s: s.end_byte - s.start_byte) if cand else n
        ent = Ent(
            rel_path,
            kind,
            sub,
            n.text.decode(),
            n.start_point[0] + 1,
            (shell.start_byte, shell.end_byte),
            (n.start_byte, n.end_byte),
        )
        key = (ent.kind, ent.name, ent.span, ent.name_span)
        if key in seen:  # method + function patterns both match one fn
            continue
        seen.add(key)
        yield ent


def aider_raw(rel_path: str):
    """aider's own get_tags_raw for the file: (name, kind, 1-based line)."""
    io = InputOutput(pretty=False, yes=True)
    # aider writes .aider.tags.cache.v4 under root; point it at a throwaway dir
    rm = RepoMap(map_tokens=0, root=tempfile.mkdtemp(), io=io)
    p = str(FIXTURE_ROOT / rel_path)
    return [(t.name, t.kind, t.line + 1) for t in rm.get_tags_raw(p, rel_path)]


def enclosing_def(ents, ref):
    """Smallest def (function/method/class) whose span contains the ref."""
    c = [
        d
        for d in ents
        if d.kind == "def"
        and d.path == ref.path
        and d.tag.split(".")[1] in ("function", "method")
        and d.span[0] <= ref.span[0]
        and ref.span[1] <= d.span[1]
    ]
    return min(c, key=lambda d: d.span[1] - d.span[0]) if c else None


def main(argv):
    for rel in argv:
        for e in scan(rel):
            print(f"{e.path}\t{e.kind}\t{e.tag}\t{e.name}\tL{e.line}\t{e.span[0]}-{e.span[1]}\tname={e.name_span[0]}-{e.name_span[1]}")


if __name__ == "__main__":
    main(sys.argv[1:])
