# Buffered Falcon proof

Implementation commit: `1707594`. Recorded 2026-09-09.

Commands, from this lab:

```sh
just test core
just test-godot
just test-simulation-wasm
just buffer-proof
```

The recording requires Metal access and FFmpeg. A restricted sandbox found no
adapter; the permitted retry recorded using Apple M2 Pro. Godot payload tests
required access to its normal user-log directory. Cargo and FFmpeg use two jobs/
threads. The recording command performs the deterministic proof before capture,
checks both fighters in every GPU readback, probes the MP4 and extracts frames.

## Results

Same 180-tick tape: jump at 20/100, early attack at 21/101, cancel at 102.
These are lab policies, not claims of Melee, PM or Ultimate fidelity.

| Observation | Window 0 | Window 8 |
| --- | --- | --- |
| Early attack expires | Tick 22 | Never |
| Buffered attack consumed | Never | Tick 25 |
| Cancellation acknowledged | Tick 102 | Tick 102 |
| GGRS Load requests | 172 | 172 |
| GGRS advances including replay | 1,384 | 1,384 |
| Exact SQLite row/mesh comparisons | 180 | 180 |
| Restored suffix ticks per method | 624 | 624 |

Restoration methods: Clone, JSON and bincode, each compared against uninterrupted
complete World state. Five checkpoints include pending, expired/consumed and
cancelled states. This is GGRS SyncTest save/load/replay, without network peers.
Production statig chart has no entry/exit actions. The earlier negative entry-
reinitialization regression remains in shared Redux tests.

`just test core` passed with 28 Falcon tests, 3 shared input tests, 1 shared
capture test and existing Redux/rollback/generator/controller checks. Generated
Godot payload tests passed. Pure simulation WASM compilation passed; browser
execution and cross-target netplay were not rerun.

## Recording

`.workflow/buffer/buffer-proof.mp4`: H.264, 960x540, 60 fps, 540 frames, 9 seconds,
735,384 bytes. Three presentation frames per simulation tick, labeled 3x slow.
SHA-256: `dcfbcc1cbc7838d1fc46a39776825479e80396660237b097216ca277bbfa62f5`.
Regenerated encodings may differ across GPU/FFmpeg versions.

Inspected `comparison.png`: tick 28, left Jump pose 5 and last expiry 22; right
Fair pose 4 and last consumption 25. Inspected `cancellation.png`: tick 102,
both Jump Squat pose 3 and cancelled. Labels derive from executed state and
actual GGRS requests. Full World/inspection traces are `buffer-proof.json`;
video metadata is `probe.json` in the same ignored artifact directory.

Pipeline: local decoded actions baked once -> shared Redux Step + snapshot-owned
shared statig buffer -> existing SQLite publication/query -> row-derived mesh ->
shared wgpu recorder -> FFmpeg. Decoding remains outside the simulation hot path.
TypeSpec defines policy/inspection IO; the existing emitter generates Rust and
Godot payloads. Assets, eligibility, tape and assertions remain game-owned.

No interactive policy picker, additional imported behavior, network transport,
deployment or allocation-performance claim. G2 remains on deck. Shared capture
is extracted; the rest of P1-P4 remains tracked in `104_tasks.md`.
