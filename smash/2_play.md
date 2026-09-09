# Play Falcon

The authoritative simulation and Falcon state machine are in this crate. The
Godot adapter is the presentation harness in the Falcon lab and calls this crate
through the built GDExtension.

Build the adapter, then launch it manually:

```sh
cd /Users/chrishafley/projects/hafley-rs/games/blender-godot-sqlite-proof/falcon-lab
just godot-build
godot --path godot -- --control
```

Controls: A/D or arrow keys move, Space jumps, J performs fair, Escape exits.
The HUD reports Rust tick, action/pose, position, hits, damage, sandbag phase,
SQL generation and snapshot status. The process remains attached to the terminal
until the user exits it.

The current Falcon transition table is generated and consumed by the Redux tick:

```sh
cargo run --features ingest --bin smash-import -- falcon
```

The command decodes the seven retained PM actions, resolves authored transition
names to imported action indices and rewrites `generated/0_chart.rs`. Missing or
duplicate action names fail generation. `cargo test --features ingest --bin
smash-import` rejects stale generated output.

The scripted verification is separate and bounded:

```sh
FALCON_CONTROL_PROOF="$PWD/.workflow/playable-demo/control-proof.json" \
  godot --headless --path godot -- --control-demo
```

It reports `CONTROL_OK ticks=300 hits=1 damage=18 replayed=120 rows_and_mesh=exact`.
