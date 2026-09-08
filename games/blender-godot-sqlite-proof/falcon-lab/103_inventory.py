"""Offline mirror inventory. Uses stdlib HTML parsing/hashing; performs no requests.

inventory(root: Path) -> dict: read each file once, validate HTML, resolve same-host
links against downloaded paths, report hashes/missing data. The mirror owns bytes;
this reader never mutates them. Live-download reports are observations, not snapshots.
"""
import argparse
import hashlib
import json
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import quote, unquote, urljoin, urlsplit

ORIGIN = "https://rukaidata.com"
FALCON = "/PM3.6/Captain Falcon/"


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = set()

    def handle_starttag(self, tag, attrs):
        for key, value in attrs:
            if key in ("href", "src") and value:
                self.urls.add(value)


def local_path(url):
    parsed = urlsplit(url)
    if parsed.scheme not in ("https", "http") or parsed.netloc != "rukaidata.com":
        return None
    path = unquote(parsed.path)
    if ".." in path.split("/") or "\\" in path or "\0" in path:
        return None
    return path


def candidates(path):
    relative = path.lstrip("/")
    return [relative, relative.rstrip("/") + ".html",
            relative.rstrip("/") + "/index.html" if relative else "index.html"]


def inventory(root):
    rows = {}
    links = set()
    falcon_expected = set()
    external = set()
    for file in sorted(root.rglob("*")):
        if not file.is_file() or file.is_symlink():
            continue
        relative = file.relative_to(root).as_posix()
        data = file.read_bytes()
        status = "present"
        refs = set()
        if data.startswith(b"\x1f\x8b"):
            status = "compressed_unparsed"
        elif file.name.endswith(".html"):
            try:
                html = data.decode("utf-8")
                if "</html>" not in html.lower():
                    status = "incomplete_html"
                else:
                    parser = Links()
                    parser.feed(html)
                    refs = parser.urls
            except (UnicodeDecodeError, ValueError):
                status = "invalid_html"
        elif not data:
            status = "empty"
        rows[relative] = dict(path=relative, url=ORIGIN + "/" + quote(relative, safe="/+"),
                              url_basis="local_path; redirect aliases resolved separately",
                              sha256=hashlib.sha256(data).hexdigest(), bytes=len(data),
                              status=status, game=relative.split("/")[0], kind=file.suffix)
        for ref in refs:
            url = urljoin(ORIGIN + "/" + quote(relative, safe="/+"), ref)
            path = local_path(url)
            if path is None:
                external.add(url)
                continue
            links.add(path)
            if relative == "PM3.6/Captain Falcon/subactions.html" and path.startswith(FALCON + "subactions/"):
                falcon_expected.add(path)

    def resolve(path):
        return next((rows[p] for p in candidates(path) if p in rows), None)

    missing = []
    for path in sorted(links):
        row = resolve(path)
        if row is None or row["status"] != "present":
            missing.append(dict(url=ORIGIN + quote(path, safe="/+"),
                                status="missing" if row is None else row["status"]))
    missing_falcon = [path for path in sorted(falcon_expected)
                      if resolve(path) is None or resolve(path)["status"] != "present"]
    return dict(version=1, network_requests=0, files=list(rows.values()),
                summary=dict(files=len(rows), bytes=sum(r["bytes"] for r in rows.values()),
                             invalid_files=sum(r["status"] != "present" for r in rows.values()),
                             discovered_links=len(links), missing_links=len(missing),
                             falcon_subactions_expected=len(falcon_expected),
                             falcon_subactions_missing=len(missing_falcon)),
                falcon_index_present="PM3.6/Captain Falcon/subactions.html" in rows,
                falcon_missing=missing_falcon, missing=missing, external=sorted(external),
                scope="Locally discovered HTML href/src links only; CSS/JS dynamic links and unlinked remote objects are not enumerated.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).parent / "../fixtures/rukaidata-mirror/rukaidata.com")
    parser.add_argument("--output", type=Path, default=Path(__file__).parent / ".workflow/mirror-inventory.json")
    args = parser.parse_args()
    if not args.root.is_dir():
        parser.error("mirror root does not exist")
    result = inventory(args.root)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(dict(result["summary"], receipt=str(args.output), network_requests=0)))
