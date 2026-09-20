"""Mechanism 2b: aider's PageRank order (RepoMap.get_ranked_tags) for a case.
Does the rank break a name-join tie? Prints defs in rank order.
usage: rank.py <case-id> <chat-file>   (chat-file is the calling file)"""
import shutil
import sys
from pathlib import Path

from aider.io import InputOutput
from aider.repomap import RepoMap

from cases import FILES
from tags import FIXTURE_ROOT

case, chat = sys.argv[1], sys.argv[2]
rm = RepoMap(map_tokens=0, root=str(FIXTURE_ROOT), io=InputOutput(pretty=False, yes=True))
abs_ = lambda f: str(FIXTURE_ROOT / f)
others = [abs_(f) for f in FILES[case] if f != chat]
ranked = rm.get_ranked_tags([abs_(chat)], others, set(), set())
for t in ranked:
    if hasattr(t, "kind"):
        print(f"def {t.rel_fname}:{t.line + 1} {t.name}" if t.kind == "def" else f"ref {t.rel_fname}:{t.line + 1} {t.name}")
    else:
        print(f"file {t}")

# RepoMap(root=...) creates .aider.tags.cache.v4 under the root; keep the tree clean
shutil.rmtree(FIXTURE_ROOT / ".aider.tags.cache.v4", ignore_errors=True)
