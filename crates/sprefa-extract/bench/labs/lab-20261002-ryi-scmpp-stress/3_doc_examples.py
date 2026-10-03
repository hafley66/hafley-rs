"""Run every ```scheme block of the scm++ guide against its x.rs and diff the printed rows against the next ``` block."""
import json, os, re, subprocess, sys, pathlib
HERE = pathlib.Path(__file__).resolve().parent
RYII = HERE.parent.parent.parent / "target" / "release" / "ryii"
DOC = HERE.parent.parent.parent / "docs" / "2_scm-with-ast-grep-relations-20260920.md"
text = DOC.read_text()
x = re.search(r"```rust\n(.*?)```", text, re.S).group(1)
(HERE / "fx" / "x.rs").write_text(x)
blocks = re.findall(r"```(\w*)\n(.*?)```", text, re.S)
ok = bad = 0
for i, (lang, body) in enumerate(blocks):
    if lang != "scheme":
        continue
    expected = next((b for l, b in blocks[i + 1:] if l == ""), None)
    (HERE / "fx" / "doc.scm").write_text(body)
    done = subprocess.run([str(RYII), "query", "--scmpp", "doc.scm", "x.rs"], cwd=HERE / "fx",
                          capture_output=True, text=True, env=dict(os.environ, RUST_LOG="warn"))
    rows = [json.loads(l) for l in done.stdout.splitlines()]
    full = "\n".join(json.dumps(r, separators=(",", ":")) for r in rows)
    texts = "\n".join(json.dumps({k: v for k, v in r.items() if k.endswith("__text")}, separators=(",", ":")) for r in rows)
    want = (expected or "").strip()
    if done.returncode == 0 and want in (full.strip(), texts.strip()):
        ok += 1
    else:
        bad += 1
        print("MISMATCH", body.strip()[:120], "\n got:", (texts or done.stderr)[:400], "\n want:", want[:400])
print(ok, "match", bad, "mismatch")
