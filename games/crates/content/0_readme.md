# Shared action content

Moved from the Falcon lab and `smash::fighters::falcon`; no app dependency.
`Action`, `Frame`, `Attack` preserve the existing Serde field order and wire shape.
`Tick`, action ordering, transitions, buffer eligibility and movement remain app-owned.

Default features expose only the Serde content types. Optional `ingest` enables
the existing brawllib_rs 0.29.0 (MIT), bincode 2 and base64 decoder/baker.

- `decode_file(&Path) -> Result<HighLevelSubaction, Box<dyn Error>>` selects any
  local subaction file. Returned upstream data retains bone poses and scripts.
- `decode_html(&str)` shares the same decoder, with explicit malformed/trailing
  payload rejection. The caller owns source bytes; decoded output owns its arrays.
- `bake(&[HighLevelSubaction]) -> Vec<Action>` copies simulation fields into owned
  immutable content. It preserves caller ordering and does not assign action IDs.

Decode and bake during offline preparation. Simulation borrows/shared-owns baked
content and mutates separate snapshots. Baked content currently omits bone poses,
which existing presentation consumers retain from decoded data. Existing game
binary format and CLI remain unchanged. No callback execution/fidelity claim.

Tests reference retained Falcon fixtures and `10_lifecycle_sources.json` provenance
under `../../blender-godot-sqlite-proof/fixtures/falcon/`. A synthetic name mutation
checks identity independence. A real second-character fixture remains pending.
No network access or fighter-specific paths occur in library implementation.

From the Falcon lab, `just test-reuse` includes the ingest tests. The existing
`falcon-import <directory>` catalogue command consumes this shared decoder.
`just test-simulation-wasm` compiles the default content feature set through Smash.
