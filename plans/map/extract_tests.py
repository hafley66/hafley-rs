#!/usr/bin/env python3
"""Extract per-test rows for the test-suite map.

Walks tests/*.rs (+support, rust_path_decl), referenced bench Rust modules, and
src #[cfg(test)] regions; finds #[test] fns; emits a TSV row per test plus the
per-commit report totals (static tests, test-code lines).
"""
import os
import re
import sys

ROOT = os.environ.get("REPO_ROOT", ".")


def scan_rust(text):
    """Yield (kind, start, end) spans for strings/chars/comments."""
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            yield ("str", i, j + 1)
            i = j + 1
        elif c == "'" and i + 1 < n and (text[i + 1].isalpha() or text[i + 1] == "_"):
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == "'":
                    break
                j += 1
            yield ("chr", i, j + 1)
            i = j + 1
        elif c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j < 0 else j
            yield ("line", i, j)
            i = j
        elif c == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i)
            j = n if j < 0 else j + 2
            yield ("blk", i, j)
            i = j
        else:
            i += 1


def mask(text):
    out = list(text)
    for kind, a, b in scan_rust(text):
        if kind in ("str", "chr"):
            for k in range(a, min(b, len(out))):
                if out[k] not in "\n":
                    out[k] = " "
    return "".join(out)


def body_span(masked, open_idx):
    depth = 0
    for i in range(open_idx, len(masked)):
        if masked[i] == "{":
            depth += 1
        elif masked[i] == "}":
            depth -= 1
            if depth == 0:
                return open_idx, i
    return open_idx, len(masked) - 1


def parse_file(path, rel):
    text = open(path, encoding="utf-8").read()
    masked = mask(text)
    header = [m.group(1).strip() for m in re.finditer(r"^\s*//!\s?(.*)$", text, re.M)]
    header = [h for h in header if h][:6]
    rows = []
    for m in re.finditer(r"#\[test\]", masked):
        line_start = text.rfind("\n", 0, m.start()) + 1
        doc_lines = []
        k = line_start
        while k > 0:
            prev_start = text.rfind("\n", 0, k - 1) + 1
            prev = text[prev_start : k - 1].strip()
            if prev.startswith("///"):
                doc_lines.insert(0, prev[3:].strip())
                k = prev_start
            else:
                break
        fm = re.compile(r"\bfn\s+([A-Za-z0-9_]+)").search(masked, m.end())
        if not fm:
            continue
        name = fm.group(1)
        brace = masked.find("{", fm.end())
        if brace < 0:
            continue
        b0, b1 = body_span(masked, brace)
        body = text[b0 : b1 + 1]
        start_line = text.count("\n", 0, m.start()) + 1
        end_line = text.count("\n", 0, b1) + 1
        rows.append(
            {
                "file": rel,
                "name": name,
                "line": start_line,
                "end_line": end_line,
                "lines": end_line - start_line + 1,
                "doc": " ".join(doc_lines)[:220],
                "header": " ".join(header)[:220],
                "body": body,
            }
        )
    return rows, len(text.split("\n"))


FAM = [
    (r"cleave_ts_oracle|cleave_ts", "cleave ts"),
    (r"cleave", "cleave"),
    (r"move", "move"),
    (r"rename", "rename"),
    (r"prolog", "prolog facts"),
    (r"markdown|doc_node|document_formats|join_documents", "documents"),
    (r"scip", "scip"),
    (r"df_|flow", "dataflow"),
    (r"cfg", "cfg"),
    (r"sqlite|ingest|blob_cache|own_blob", "storage"),
    (r"daemon|http|server_modes|client", "daemon/http"),
    (r"tracing|trail|drain|origin_column", "tracing/trail"),
    (r"query", "query"),
    (r"witness|resolve_witness|tier_decline|unresolved", "resolve contract"),
    (r"tsi|type_ladder|typegraph|kind_vocab|type_plane|type_refs|type_edges", "type facts"),
    (r"graph", "graph"),
    (r"crawl|kinks|defects|corpus_gaps|residual", "crawl defects"),
    (r"module_plane|module_edges|modules|module_resolve|mod_file_edges|mod_scope|cargo_metadata|package_edges|source_tree|crate_scope|export_cycle", "module plane"),
    (r"call", "call plane"),
    (r"receiver", "receiver typing"),
    (r"specifiers|binding_legs|member_calls|iface|promoted|multihop|closure_mirror|init_receivers|property_arrow|destructured|namespace_members|generic_args|qualified_type|type_alias|variant|impl_owner|traits|trait_blob|collapsed_span|mbe|macro", "binding shapes"),
    (r"fast_scm|stratify|scmpp", "fast scm"),
    (r"ratchet|parity|baseline|capability|golden|snapshot|resume", "oracles/parity"),
    (r"quality_gate|hooks_query|dogfood|help|identity|build_metadata|extract_lang|cli", "cli/dogfood"),
    (r"throughput|scaling|grind|slow|n_plus_one|large_file|size_skip|bench|wall", "perf budgets"),
    (r"rust", "rust misc"),
    (r"\bts|_ts|^ts", "ts misc"),
    (r"go", "go misc"),
    (r"kotlin", "kotlin misc"),
    (r"python|py_", "python misc"),
]


def bucket(fname):
    stem = re.sub(r".*/", "", fname).replace(".rs", "")
    for pat, name in FAM:
        if re.search(pat, stem):
            return name
    return "misc"


def humanize(name):
    return " ".join(w for w in name.split("_") if w not in ("a", "the", "is", "of", "and"))


def classify(row):
    body = row["body"]
    feats = []
    if re.search(r"insta::|assert_json_snapshot|assert_snapshot", body):
        feats.append("snapshot")
    if re.search(r"\bassert_eq!\b", body):
        feats.append("assert_eq")
    if re.search(r"\bassert!\b", body):
        feats.append("assert!")
    if re.search(r"Command::new", body):
        feats.append("spawn")
    if re.search(r"fixture_runner|Fixture::from_dir|fixture\(", body):
        feats.append("fixture_runner")
    fixes = set(re.findall(r"tests/fixtures/([A-Za-z0-9_./-]+)", body))
    fixes |= set(re.findall(r'from_dir\("([A-Za-z0-9_./-]+)"\)', body))
    fixes |= set(re.findall(r'fixture\("([A-Za-z0-9_./-]+)"\)', body))
    fixes |= set(re.findall(r'run\("([A-Za-z0-9_./-]+)"', body))
    cmds = set()
    for m in re.finditer(r"CARGO_BIN_EXE_(\w+)", body):
        cmds.add(m.group(1))
    for m in re.finditer(r"Command::new\(\s*([^)]{0,60})", body):
        cmds.add(m.group(1).strip()[:40])
    row["concern"] = bucket(row["file"])
    row["assertion"] = "+".join(feats)
    row["fixture"] = ";".join(sorted(fixes))[:180]
    row["command"] = ";".join(sorted(cmds))[:180]
    row["claim"] = row["doc"] if row["doc"] else humanize(row["name"])
    return row


def main():
    targets = []
    tdir = os.path.join(ROOT, "crates/sprefa-extract/tests")
    for name in sorted(os.listdir(tdir)):
        p = os.path.join(tdir, name)
        if name.endswith(".rs") and name != "all.rs":
            targets.append((p, f"tests/{name}"))
    for sub in ("rust_path_decl", "support"):
        d = os.path.join(tdir, sub)
        if os.path.isdir(d):
            for name in sorted(os.listdir(d)):
                p = os.path.join(d, name)
                if name.endswith(".rs"):
                    targets.append((p, f"tests/{sub}/{name}"))
    bdir = os.path.join(ROOT, "crates/sprefa-extract/bench")
    if os.path.isdir(bdir):
        for dirpath, _dirs, files in os.walk(bdir):
            for f in files:
                if f.endswith(".rs"):
                    p = os.path.join(dirpath, f)
                    targets.append((p, f"bench/{os.path.relpath(p, bdir)}"))
    for rel_root, crate in [
        ("crates/sprefa-extract/src", "src"),
        ("crates/hafley_scm/src", "scm"),
    ]:
        for dirpath, _dirs, files in os.walk(os.path.join(ROOT, rel_root)):
            for f in files:
                if f.endswith(".rs"):
                    p = os.path.join(dirpath, f)
                    text = open(p, encoding="utf-8").read()
                    if "#[test]" in text:
                        targets.append((p, f"{crate}/{os.path.relpath(p, ROOT)}"))
    out = []
    file_lines = {}
    for p, rel in targets:
        rows, total = parse_file(p, rel)
        file_lines[rel] = total
        for row in rows:
            out.append(classify(row))
    tsv = os.path.join(ROOT, "crates/sprefa-extract/bench/test-map/tests.tsv")
    os.makedirs(os.path.dirname(tsv), exist_ok=True)
    cols = ["file", "name", "line", "end_line", "lines", "concern", "assertion", "command", "fixture", "claim", "doc", "header"]
    with open(tsv, "w", encoding="utf-8") as fh:
        fh.write("\t".join(cols) + "\n")
        for row in out:
            fh.write("\t".join(str(row[c]).replace("\t", " ").replace("\n", " ") for c in cols) + "\n")
    # per-commit report totals: tests/ + src cfg(test) spans + referenced bench Rust modules
    tests_code = sum(
        n for f, n in file_lines.items() if f.startswith("tests/") or f.startswith("bench/")
    )
    src_cfg = 0
    for rel_root in ("crates/sprefa-extract/src", "crates/hafley_scm/src"):
        for dirpath, _dirs, files in os.walk(os.path.join(ROOT, rel_root)):
            for f in files:
                if not f.endswith(".rs"):
                    continue
                p = os.path.join(dirpath, f)
                text = open(p, encoding="utf-8").read()
                masked = mask(text)
                for m in re.finditer(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", masked):
                    mod = re.compile(r"\bmod\s+\w+[^\{]*\{").search(masked, m.end())
                    if not mod:
                        continue
                    brace = masked.find("{", mod.end() - 1)
                    b0, b1 = body_span(masked, brace)
                    src_cfg += text.count("\n", b0, b1) + 1
    bench_ref = 0
    for dirpath, _dirs, files in os.walk(tdir):
        for f in files:
            if not f.endswith(".rs"):
                continue
            text = open(os.path.join(dirpath, f), encoding="utf-8").read()
            for m in re.finditer(r'#\[path\s*=\s*"(\.\./bench/[^"]+)"\]', text):
                bp = os.path.normpath(os.path.join(dirpath, m.group(1)))
                if os.path.isfile(bp):
                    bench_ref += len(open(bp, encoding="utf-8").read().split("\n"))
    static_tests = len(out)
    code_lines = tests_code + src_cfg + bench_ref
    with open(os.path.join(os.path.dirname(tsv), "totals.tsv"), "w", encoding="utf-8") as fh:
        fh.write(f"static_tests\t{static_tests}\n")
        fh.write(f"test_code_lines\t{code_lines}\n")
        fh.write(f"tests_dir_lines\t{tests_code}\n")
        fh.write(f"src_cfg_test_lines\t{src_cfg}\n")
        fh.write(f"bench_referenced_lines\t{bench_ref}\n")
    print(f"{static_tests} tests, {code_lines} test-code lines -> {tsv}")


if __name__ == "__main__":
    main()
