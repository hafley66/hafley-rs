# Character art tools

From games/kneeman:

```sh
just art-import friend_capture.zip
just import
just run
```

The capture page ships at `/game3/poses/index.html`. Use camera capture or Add photos for the
selected clip. Idle is required; missing clips use the game's existing fallback chain. ZIP export
contains PNG frames and character.json. Captures persist in that browser's IndexedDB. No photo
upload endpoint is used. Captures are raw photos; automatic background removal is not present.

The importer validates paths, required PNGs, clip metadata and size limits before writing. It adds
one roster entry and preserves all others. An existing character directory/name is rejected;
use a new name for another revision. Run game3-publish to include imported art in the deployed game.

## Rivals of Aether workshop clips

```sh
python3 -m venv tools/.venv
tools/.venv/bin/pip install -r tools/requirements.txt
ART_PYTHON=tools/.venv/bin/python3 just steam-login
ART_PYTHON=tools/.venv/bin/python3 just roa-search "character name"
ART_PYTHON=tools/.venv/bin/python3 just roa-get WORKSHOP_ID
# Add a new [[pack]] to tools/packs.toml pointing at the downloaded sprites directory.
ART_PYTHON=tools/.venv/bin/python3 just packs
just import
```

Steam login and downloads are explicit operator actions. Tokens retain the original tool's
external credential location; they are not copied into this repository. Local PNG strips can be
imported without Steam. The converter imports animation images and clip metadata. It preserves
unrelated roster entries and rejects overwriting a registered character.

`just refs` generates optional pose ghosts from the imported Falcon strips. `slice_sheet.py`
retains the original irregular/grid sheet tool. Generated ghosts remain local until deliberately
added to the deployed art set. Use art you have permission to redistribute.

Pillow is required for conversion/import/tests. Existing pins are in requirements.txt; scripts
perform no implicit package installation. `ART_PYTHON` selects the Python interpreter.

Source: kneeman-lines/0_rust_v1_ship/tools at 83085cd0. Functional changes here: new Game3 output
paths, explicit dependency setup, roster-preserving validated installation, gallery input, removal
of the old unauthenticated upload action, and a fixed camera/gallery frame resolution.
