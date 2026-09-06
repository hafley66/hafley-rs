"""Install a pose-capture ZIP into Game3 without replacing existing characters.

python3 tools/0_import_character.py friend_capture.zip [--assets PATH]
Revised captures use a new directory/name; existing art is never silently overwritten.
"""
import argparse
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import tempfile
import zipfile

ASSETS = Path(__file__).resolve().parents[1] / "app/godot/assets"
MAX_BYTES = 128 * 1024 * 1024


def relative_name(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*", value):
        raise ValueError(f"Unsafe asset name: {value!r}")
    return value


def validate(character, files):
    from PIL import Image
    relative_name(character["dir"])
    sheet = character.get("sheet")
    if sheet not in ("poses", "strip"):
        raise ValueError("Expected poses or strip sheet")
    prefix = relative_name(character["prefix"]) if sheet == "poses" else None
    if prefix and "/" in prefix:
        raise ValueError("Prefix must be a single name")
    clips = character["clips"]
    if not clips or len(clips) > 256 or not any(c["name"] == "idle" for c in clips):
        raise ValueError("A character needs an idle clip and at most 256 clips")
    if len({c["name"] for c in clips}) != len(clips):
        raise ValueError("Duplicate clip names")
    required = set()
    for clip in clips:
        relative_name(clip["name"])
        frames, fps = clip["frames"], clip["fps"]
        if type(frames) is not int or not 1 <= frames <= 512 or not math.isfinite(fps) or not 0 < fps <= 240:
            raise ValueError("Invalid clip frame count or fps")
        if type(clip.get("loop")) is not bool or not clip["files"]:
            raise ValueError("Invalid clip loop/files")
        if sheet == "poses" and len(clip["files"]) != frames:
            raise ValueError("Pose frame count differs from files")
        for stem in clip["files"]:
            relative_name(stem)
            if "/" in stem:
                raise ValueError("Frame names must be flat")
            required.add(f"{prefix}_{stem}.png" if prefix else f"{stem}.png")
    if set(files) != required or len(files) > 1024 or sum(map(len, files.values())) > MAX_BYTES:
        raise ValueError("Missing/unreferenced frames or pack exceeds limits")
    for name, data in files.items():
        with Image.open(io.BytesIO(data)) as image:
            if image.format != "PNG" or max(image.size) > 4096:
                raise ValueError(f"Invalid/oversized PNG: {name}")
            image.verify()
    for key, default in [("scale", 1.0), ("offset_y", 0.0)]:
        value = character.get(key, default)
        if not math.isfinite(value) or (key == "scale" and value <= 0):
            raise ValueError(f"Invalid {key}")


def install(character, files, assets=ASSETS):
    """Validate first, stage locally, then append one roster entry under an exclusive lock."""
    validate(character, files)
    assets = Path(assets).resolve()
    assets.mkdir(parents=True, exist_ok=True)
    lock = assets / ".import-lock"
    lock.mkdir()  # Concurrent imports fail rather than lose another roster update.
    try:
        destination = assets / character["dir"]
        if not destination.resolve().is_relative_to(assets):
            raise ValueError("Asset directory escapes through a symlink")
        roster_path = assets / "roster.json"
        roster = json.loads(roster_path.read_text()) if roster_path.exists() else {"characters": []}
        if destination.exists() or any(c["dir"] == character["dir"] for c in roster["characters"]):
            raise ValueError("Character already exists; choose a new directory/name")
        with tempfile.TemporaryDirectory(prefix=".import-", dir=assets) as scratch:
            stage = Path(scratch) / "character"
            stage.mkdir()
            for name, data in files.items():
                (stage / name).write_bytes(data)
            roster["characters"].append(character)
            staged_roster = Path(scratch) / "roster.json"
            staged_roster.write_text(json.dumps(roster, indent=2) + "\n")
            destination.parent.mkdir(parents=True, exist_ok=True)
            os.replace(stage, destination)
            try:
                os.replace(staged_roster, roster_path)
            except BaseException:
                shutil.rmtree(destination)  # Only the directory created by this invocation.
                raise
        return destination
    finally:
        lock.rmdir()


def read_capture(path):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) > 2048 or sum(e.file_size for e in entries) > MAX_BYTES:
            raise ValueError("Capture exceeds import limits")
        if len({e.filename for e in entries}) != len(entries):
            raise ValueError("Duplicate ZIP entries")
        for entry in entries:
            name = PurePosixPath(entry.filename)
            if name.is_absolute() or ".." in name.parts or "\\" in entry.filename:
                raise ValueError("Unsafe ZIP path")
        character = json.loads(archive.read("character.json"))
        directory = relative_name(character["dir"])
        prefix = f"assets/{directory}/"
        files = {}
        for entry in entries:
            if entry.is_dir() or entry.filename in ("character.json", "README.txt"):
                continue
            if not entry.filename.startswith(prefix):
                raise ValueError("ZIP asset is outside the character directory")
            name = entry.filename[len(prefix):]
            if "/" in name or not name.endswith(".png"):
                raise ValueError("Expected flat PNG frames")
            files[name] = archive.read(entry)
        return character, files


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--assets", type=Path, default=ASSETS)
    args = parser.parse_args()
    print(install(*read_capture(args.capture), assets=args.assets))
