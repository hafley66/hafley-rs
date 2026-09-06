import importlib
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

from PIL import Image

capture = importlib.import_module("0_import_character")


class ImportTests(unittest.TestCase):
    def test_workshop_strip_and_grid_imports_share_the_roster_contract(self):
        import fetch_packs
        image = io.BytesIO()
        Image.new("RGBA", (12, 8), (0, 255, 0, 255)).save(image, format="PNG")
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            assets = root / "assets"
            for name, builder, pack, files in [
                ("strip", fetch_packs.build_strip_pack, {"name": "strip"}, {"idle_strip3.png": image.getvalue()}),
                ("grid", fetch_packs.build_grid_pack, {"name": "grid", "cols": 3, "rows": 2,
                 "rows_order": ["idle", "walk"]}, {"sheet.png": image.getvalue()}),
            ]:
                stage = root / name
                stage.mkdir()
                character = builder(pack, files, stage)
                capture.install(character, {p.name: p.read_bytes() for p in stage.glob("*.png")}, assets)
            roster = json.loads((assets / "roster.json").read_text())["characters"]
            self.assertEqual([(c["dir"], c["sheet"], [(x["name"], x["frames"]) for x in c["clips"]])
                              for c in roster], [("strip", "strip", [("idle", 3)]),
                                                 ("grid", "strip", [("idle", 3), ("walk", 3)])])
            with Image.open(assets / "grid" / "walk_strip3.png") as frame:
                self.assertEqual(frame.size, (12, 4))

    def test_capture_roundtrip_preserves_roster_and_rejects_invalid_imports(self):
        image = io.BytesIO()
        Image.new("RGBA", (2, 2), (255, 0, 0, 255)).save(image, format="PNG")
        character = {"dir": "friends/test", "sheet": "poses", "prefix": "test", "scale": 1.0,
                     "offset_y": 0.0, "clips": [{"name": "idle", "files": ["idle"],
                     "frames": 1, "fps": 6, "loop": True}]}
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            assets = root / "assets"
            assets.mkdir()
            old = {"characters": [{"dir": "existing"}], "other": "preserved"}
            (assets / "roster.json").write_text(json.dumps(old))
            archive = root / "capture.zip"
            with zipfile.ZipFile(archive, "w") as out:
                out.writestr("character.json", json.dumps(character))
                out.writestr("assets/friends/test/test_idle.png", image.getvalue())
            result = capture.install(*capture.read_capture(archive), assets=assets)
            self.assertEqual((result / "test_idle.png").read_bytes(), image.getvalue())
            expected = {"characters": old["characters"] + [character], "other": "preserved"}
            self.assertEqual(json.loads((assets / "roster.json").read_text()), expected)
            with self.assertRaisesRegex(ValueError, "already exists"):
                capture.install(*capture.read_capture(archive), assets=assets)
            with zipfile.ZipFile(archive, "a") as out:
                out.writestr("../escape.png", image.getvalue())
            with self.assertRaisesRegex(ValueError, "Unsafe ZIP"):
                capture.read_capture(archive)
            with self.assertRaisesRegex(ValueError, "Missing/unreferenced"):
                capture.install(character, {}, assets=assets)
            self.assertEqual(json.loads((assets / "roster.json").read_text()), expected)
            self.assertFalse((root / "escape.png").exists())
            self.assertFalse((assets / ".import-lock").exists())


if __name__ == "__main__":
    unittest.main()
