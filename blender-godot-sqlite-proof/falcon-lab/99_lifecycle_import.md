# PM Falcon lifecycle import coverage

Source: Rukaidata PM3.6 Captain Falcon, retrieved 2026-09-08. Exact HTML bytes are
pinned in `../fixtures/falcon/10_lifecycle_sources.json`. `just import-check`
checks SHA-256 pins, decodes each payload fully, and prints JSON including scripts.
`just import-fetch` restores missing new payloads; it does not replace pinned files.

Decoder API: `brawllib_rs::high_level_fighter::{HighLevelSubaction,HighLevelFrame}`,
crate 0.29.0 (MIT), serde/bincode 2 standard. Its mandatory native wgpu/winit
dependencies remain on the offline side. Source assets have separate permissions.
Local vendor source inspected: `high_level_fighter.rs`, especially the
attribute-driven frame-count overrides and per-frame script evaluation.

| Subaction | Decoded frames | IASA | Imported landing flag |
| --- | ---: | ---: | --- |
| Wait1 | 61 | absent | false |
| JumpF | 36 | absent | false |
| AttackAirF | 40 | 35 | true at zero-based 6–34 |
| JumpSquat | 4 | absent | false |
| Fall | 9 | absent | false |
| LandingAirF | 19 | 19 | false |
| LandingHeavy | 3 | 3 | false |

`AttackAirF.landing_lag` is 19. All seven payloads report `bad_interrupts=false`.
The JumpSquat, Fall, LandingAirF and LandingHeavy main script event lists are empty.
The decoder obtains squat/landing frame counts from fighter attributes before
serializing the extracted payload. The payload alone does not contain those
attribute records or the game's common movement callbacks.

## Runtime mapping

`35_runtime::bake` preserves interruptibility, landing-enable flags, IASA and
landing-lag metadata alongside existing hitboxes and positions. Poses/capsules,
timings and script-evaluated flags are mechanically reused. The runtime uses frame
counts for squat/recovery duration and per-frame flags for aerial interruption
and landing selection. IASA and landing-lag values remain available as metadata;
the landing-length agreement is asserted at load.

The seven-action controlled path uses the existing pure Rust World and snapshot,
Parry contact, Rapier sandbag, SQLite rows, generated payload and mesh receipt.
Ground contact precedes aerial input. Landing blocks jump until recovery finishes;
holding a rejected jump creates no fresh input edge. No L-cancel input exists.
Legacy three-action fixtures retain their previous advancement path.

Lab-authored policy still supplies action transitions, the parabolic jump curve,
horizontal motion and ground plane. Full common callbacks, collision precedence,
short hop/fastfall, ledges, facing, hitlag, and source-game traces remain unsupported
or unverified. ECB, velocity modifiers and other script effects are not newly
executed. No Melee callback code was imported or mixed into PM behavior here.
Native/browser equality proves this implementation's parity, not PM equivalence.
The HUD explicitly labels PM timings/poses and lab transitions/movement.

## Acceptance trajectory

Ticks are zero-based. Changed actions:

```text
0 idle
60 squat -> 64 jump -> 74 fair -> 114 fall -> 117 heavy landing -> 120 idle
180 squat -> 184 jump -> 210 fair -> 237 fair landing -> 256 idle
270 squat -> 274 jump
```

The first fair connects at tick 87 for 18 damage. The second fair misses and lands
during its landing-enable window. Native tests assert this sequence, landing
input rejection, and exact replay from snapshots at transition boundaries.
The browser harness asserts every transition and records a landing screenshot
and H.264 MP4. Reproduce with `just record control` or the web verification flow.

The source-game trace comparison and executable common-callback dependency closure
remain unchecked in `92_web_import_plan.md`.

## Executed evidence

Implementation committed as `bb50078`. On 2026-09-08, 27 Rust tests, generator
checks, controller tests, native Godot input/typed-payload checks, Web export,
local Chromium and production Chromium acceptance passed. Both browsers checked
all 14 transition entries above, 300 native-matching presentation frames, 120
restored/replayed states, keyboard and synthetic touch input, with no runtime
errors recorded. The production landing screenshot was inspected.

Live proof: https://hafley.codes/game3/?demo=1

Saved local artifacts (ignored build directory):

```text
94_web/build/proofs/bb50078/proof.mp4
94_web/build/proofs/bb50078/receipt.json
94_web/build/proofs/bb50078/1a_landing.png
```

MP4: H.264, 960×540, 13.4 seconds, 592879 bytes. Production receipt retains the
original temporary recording paths; copies above preserve these outputs locally.
Prior Game3 backup: `/var/www/smash-godot-game3-backup-20260908210046980`.
Publication verified artifact hashes and unchanged protected `/game/` files.
Log: `/private/tmp/falcon-lifecycle-publish.log`.
