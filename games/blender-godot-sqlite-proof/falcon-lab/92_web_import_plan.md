# Falcon lab: browser, behavior import, and photo characters

Recorded 2026-09-08. User-approved direction; unchecked work remains unimplemented.

## Destination and protection

- Publish **this Falcon demo/work** at `https://hafley.codes/game3/`.
- The user explicitly authorizes replacing the existing `/game3/` deployment.
- **Do not modify `/game/`.** The existing personal-site game remains protected until
  the user explicitly authorizes its replacement in a later task.
- The existing Game3 configuration resolves to `root@hafley.codes` and
  `/var/www/smash-godot-game3/`. The protected game uses `/var/www/smash-godot/`.
  Resolve and check exact targets before publishing; record protected artifact
  hashes before and after. Do not broaden deletion, routing, or server changes.
- `hafley-rs-game-runtime/games/kneeman/app/deploy` and
  `hafley-rs-game-runtime/tools/godot-web` contain an existing Godot/Emscripten
  build/export/publish implementation to inspect for reuse. That checkout has
  unrelated uncommitted changes. Preserve them. It is a deployment reference,
  not the application selected for this replacement.

## End state

A compact, measured Rust/WASM simulation with Godot Web presentation. Reusable
world/fighter systems should support exploration inspired by Terraria, Smash,
Lovers in a Dangerous Spacetime, Zelda, and Kirby 64 minigames. This describes
the desired range of content, not a commitment to implement every game now.

- Keep authoritative simulation, state machines, and rollback state in Rust.
- Keep Godot rendering, audio, device input, menus, and asset-import UI outside
  authoritative simulation.
- TSP owns common IO types, identifiers, constants, commands, and effect payloads;
  emit target-specific adapters. Use the active TypeSpec skill before schema work.
- Preserve owned boundary traits and the memory/SQLite adapter strategy.
- Target bounded/recycled storage and batched transfers. Measure allocations,
  copies, startup, download size, and tick cost before making optimization claims.
- Preserve deterministic assertions and reproducible, inspected H.264 MP4s as
  the exploration workflow. On-screen labels must report executed state.

## Existing behavior comes through importers

The user wants existing Smash state machines and behavior brought into the
bespoke system, minimizing manual restatement of established gameplay rules.

- Extract state/callback tables, timelines, parameters, collision data, and
  behavior references mechanically from version-pinned sources.
- Executable guards, transition precedence, physics callbacks, and side effects
  require a compatible execution path or verified translation of supported
  source constructs. A list of state IDs alone does not establish equivalence.
- Preserve source revision, symbol, and conversion diagnostics with imported
  definitions. Report unsupported behavior rather than approximating silently.
- First bounded dependency closure: Falcon idle, jump squat, jump, forward air,
  fall, landing, and return to idle, including shared fighter routines.
- Produce a coverage report before filling gaps: extracted data, executable
  behavior reused/translated, and unresolved dependencies.
- Melee decomp code and PM assets are different version sources. Record exact
  provenance and validate against the chosen source game's traces. Existing lab
  golden tests prove regression safety, not Melee/PM equivalence.

Source leads inspected:

- `https://github.com/doldecomp/melee/blob/master/src/melee/ft/chara/ftCaptain/ftCa_Init.c`
  exposes Falcon's motion-state callback table. Pin a revision before importing.
- Local `games/smash/vendor/brawllib_rs/src/high_level_fighter.rs` exposes
  `HighLevelSubaction::{iasa, landing_lag, scripts}` and frame interruptibility,
  landing flags, ECB, and velocity changes. The lab's baked `Action/Frame` currently
  preserves fewer fields. Audit exact payload coverage before using those fields.

## Photo-to-character workflow

Desired asset tool: a **3D model viewer with mobile photo input**, with a single
photo as the minimum input. A full scan should not be required for the initial flow.

The user already built a "take a photo and add yourself to Smash" tool for friends
in older repositories, and researched phone scans/AI-assisted reconstruction.
Find and reuse that implementation before adding a replacement. The existing
Kneeman pose-capture/import tools are one discovered lead, not yet confirmed as
the exact older tool. **2D mode and photo/sprite cutouts attached to 3D collision
capsules are acceptable outcomes.** Full 3D reconstruction remains optional.

1. Choose a Smash character/rig and an animation-frame span.
2. Preview and align the selected pose/projection with an uploaded or captured photo.
3. Use projected character geometry to define a silhouette/mask and crop the photo.
4. Apply the captured appearance to the existing rigged model, then preview animation
   across the selected span. The exact mapping/reconstruction method remains to select.
5. Import the resulting appearance/assets without changing the authoritative
   fighter behavior. Additional frame crops/sprite outputs may use the same selection.

Research current maintained tools for masking, projection/UV texture transfer,
and optional photo-conditioned 3D reconstruction before choosing an implementation.
Keep these outcomes explicit: a silhouette crop, projected texture on an existing
mesh, and newly reconstructed geometry are different outputs. One photograph
does not observe hidden surfaces; require an explicit fallback policy for them.
Preserve photo provenance and local processing by default. External photo uploads,
paid inference, and redistribution permissions require their own authorization.

## Ordered execution and acceptance

- [x] Native typed Rust/Godot frame, status, and receipt boundary: `c023bb5`.
- [x] Controlled-input demo and recording: `f40512d`.
  26 Rust tests, 360 golden worlds, keyboard events, SQL/mesh equality, and 120
  restored/replayed states passed. Demo: one hit at 91 for 18 damage; second fair misses.
- [x] Pure simulation compile check for `wasm32-unknown-unknown` passed on 2026-09-08,
  including the current Parry/Rapier dependencies. Log:
  `/private/tmp/falcon-core-wasm-check.log`. This is compilation only.
- [x] Verify the actual Godot Web/Emscripten extension path for this lab. Avoid
  bundling native recording/decoder dependencies when an offline import suffices.
- [x] Execute browser input, hit, SQL boundary, snapshot replay, and presentation
  assertions; measure artifact sizes and inspect the browser output.
- [x] Deploy the exact verified demo to `/game3/`; verify production behavior,
  artifact identity, and unchanged protected `/game/` artifacts.
- [ ] Pin and inventory Falcon's jump/fair/landing behavior dependency closure;
  create the first importer coverage receipt and source-trace comparison.
- [x] Import the PM pose/timing/landing-flag subset and record both landing paths
  and recovery into a third jump: `bb50078`, coverage in `99_lifecycle_import.md`.
  Native and production browser acceptance passed. Common callbacks and the
  source-game trace comparison remain pending.
- [ ] Build the bounded model/animation-span/photo alignment and silhouette proof,
  after selecting existing tools and deciding texture/hidden-surface policy.

Godot Web build and local Chromium runtime proof completed in `3c452b6`, with
deployment compatibility and event-driven acceptance fixes in `f99fc15` and
`6b6395c`. Production acceptance and recovery evidence: `96_web_results.md`.
The partial PM data importer is implemented; executable common-callback translation,
source-game trace comparison, and photo pipeline remain pending.
