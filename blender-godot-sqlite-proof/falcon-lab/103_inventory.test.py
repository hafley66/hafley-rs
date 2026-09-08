import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("inventory", Path(__file__).with_name("103_inventory.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class InventoryTests(unittest.TestCase):
    def test_missing_truncated_repaired_and_unchanged_local_data(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fighter = root / "PM3.6/Captain Falcon"
            (fighter / "subactions").mkdir(parents=True)
            (fighter / "subactions.html").write_text('''<html>
              <a href="/PM3.6/Captain%20Falcon/subactions/Wait1.html">idle</a>
              <a href="/PM3.6/Captain Falcon/subactions/JumpF.html">jump</a>
              <a href="/PM3.6/Captain Falcon/subactions">index alias</a>
              <script src="https://elsewhere.test/tool.js"></script></html>''')
            wait = fighter / "subactions/Wait1.html"
            wait.write_text("<html>partial")
            before = module.inventory(root)
            self.assertEqual(before["summary"], dict(files=2, bytes=sum(p.stat().st_size for p in root.rglob("*.html")),
                invalid_files=1, discovered_links=3, missing_links=2,
                falcon_subactions_expected=2, falcon_subactions_missing=2))
            wait.write_text("<html>complete</html>")
            (fighter / "subactions/JumpF.html").write_text("<html>jump</html>")
            after = module.inventory(root)
            self.assertEqual((after["missing"], after["falcon_missing"], after["network_requests"]), ([], [], 0))
            self.assertEqual(after, module.inventory(root))
            self.assertEqual(after["external"], ["https://elsewhere.test/tool.js"])
            old = {row["path"]: row["sha256"] for row in before["files"]}
            new = {row["path"]: row["sha256"] for row in after["files"]}
            self.assertNotEqual(old[wait.relative_to(root).as_posix()], new[wait.relative_to(root).as_posix()])

    def test_compressed_bytes_and_unsafe_urls_are_not_complete_html(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "index.html").write_bytes(b"\x1f\x8bcompressed")
            self.assertEqual(module.inventory(root)["files"][0]["status"], "compressed_unparsed")
            (root / ".html").write_text("<html>unnamed partial")
            rows = {row["path"]: row for row in module.inventory(root)["files"]}
            self.assertEqual(rows[".html"]["status"], "incomplete_html")
        for url in ["https://elsewhere.test/a", "file:///tmp/a", "https://rukaidata.com/%2e%2e/a"]:
            self.assertIsNone(module.local_path(url))


if __name__ == "__main__":
    unittest.main()
