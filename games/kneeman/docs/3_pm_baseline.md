# Project M reference baseline

Working target: original Project M 3.6, following the recovered `pm-falcon-kit.md` title.
This version is a working assumption pending an authenticated release artifact. PM-CC,
3.6.1 development builds, Project+ and Melee retain distinct labels in behavior receipts.

## Pinned text reference

Repository: https://github.com/Project-M-CC/Project-M-CC
Revision: `6e63ffa920d45e9d0236edbec4bf43057e7b3e2d`, dated 2016-07-26.
Local sparse checkout: `/Users/chrishafley/projects/kneeman-lines/7_project_m_cc`.
Only README and three text codesets are materialized. No game binaries or character assets
were downloaded, executed or added to Game3. The README describes a community build based
on 3.6.1 with optional 3.6 fighter reverts; its root character assets cannot be assumed to
match the original 3.6 release.

| File under `[Dev Resources]` | Encoding | SHA-256 |
| --- | --- | --- |
| codes-3_6.txt | ASCII | 0a2f08b47e06306dd675dc901c0e35c88fddd1b82fd95ad5b43d68004902142a |
| codes-3_6-wifi.txt | ASCII | 2e106e3aeb2ce7afdddf03d37580e4c8b8d780b818a013f1fb5c4a9ce76d4ab7 |
| codes-3_61.txt | UTF-16LE | 1065affec29049d4096ab802f5cd69f4e38cc24c6a0b4ea21f6920ffab9126ba |

The archived 3.6 text begins with `Unknown Code` and has no mechanic labels. Labels below
come from the annotated 3.6.1 file. Complete consecutive code-row blocks match the archived
3.6 file exactly after removing the annotation prefix. This proves shared bytes in these
files, without establishing complete release identity, runtime timing or fighter attributes.

| Candidate mechanic | 3.6.1 label line | Code rows | Matching 3.6 line | Game3 inspection |
| --- | ---: | ---: | ---: | --- |
| Last-frame jump direction | 3064 | 10 | 2766 | `za_warudo.rs` JumpSquat reads direction throughout squat and at takeoff; exact source semantics unverified |
| Jump-canceled grab | 3803 | 9 | 3283 | Provisional Game3 grab transition added; exact PM priority/momentum remain unverified |
| L-canceling, part 1 | 4333 | 46 | 3781 | One patch part only; cannot establish full behavior or window |

Reproduce from `games/kneeman`:

```sh
node tools/0_pm_codes_receipt.mjs /Users/chrishafley/projects/kneeman-lines/7_project_m_cc
```

The checker rejects unexpected source hashes and requires one exact occurrence of each
complete block. It reads text only and emits line receipts; it does not interpret Gecko/PSA.

## Next mechanic: jump-canceled grab

1. Interpret the nine-row patch with the corresponding Brawl action data and addresses.
2. Determine input priority, permitted squat ticks, airborne boundary and momentum behavior.
3. Record Falcon run -> jump -> grab input sequences, including one tick before/at/after takeoff.
4. Assert state transitions and full-state replay in Game3; then expose the fixture in the debugger.
5. Compare a reference execution before marking PM parity. A matching transition name is insufficient.

Game3 implementation checkpoint: the grab input is already filtered by item/holding context
before `transition`. During JumpSquat it now enters the same Grab state as a standing grab,
including the existing 0.25 horizontal-velocity multiplier, and clears movement/aerial lanes.
Grab takes priority over squat up-smash/takeoff in this provisional Game3 rule. These choices
are implementation policy, not a decoded claim about the PM patch. The patch delegates to
Brawl routines at 0x80FA973C/0x80FAD96C; their priority and momentum semantics remain unresolved.

The new regression failed at grab tick 1 before implementation. It now covers each current
three-frame squat tick plus the first airborne tick: squat accepts, airborne rejects, and
90-tick full-state replay matches for each case with restoration at tick 30. Browser script
jump at 60, grab at 61, freeze at 62 reports Grab/Stand at y=410/410 with no exceptions.
Follow-up regression covers four 100-tick run -> jump -> grab sequences: empty/held gun,
each with/without simultaneous up-attack. Empty-hand grabs stay grounded with reduced positive
forward velocity and override the competing aerial input. Held guns detach as armed throws,
retain owner 0 for self-hit exclusion, and leave the fighter in JumpSquat. Every state matches
replay, including snapshot restoration at tick 30. The first test attempt incorrectly expected
an unowned thrown item; `item::throw_item` documents retained ownership, so that assertion was
corrected without changing gameplay. 432 game + 70 shell tests pass.
Debugger fixture selection and step/verify/restore are tested and published through de86527;
docs/2_next.md records the 45-tick debugger receipt and production tick-62 Grab/Stand smoke
test. A PM runtime comparison, exact momentum values and additional conflicts such as
grab/shield/special remain to be tested.

## Game3 Falcon Dive execution checkpoint

Grounded semantic-input coverage exposed startup cancellation: Dive cleared `ground_plat`
while its startup velocity was zero, then the special floor sweep immediately changed it
to Landing. The observed path was SpecialU -> Landing -> Stand with zero damage and no rise.
Support now remains until the authored launch frame. Airborne startup retains its existing
behavior. No PM frame values or hitboxes were inferred or changed.

Terrain & replay now offers Dive catch and Dive whiff. Each starts at settled tick 60,
with Falcon/Lucas and opponent separation 90 or 300 pixels, presses up-special for one
wire-quantized input tick, and records 180 ticks. Native expectations:

- Catch: SpecialU -> GrabHold -> Air -> Landing -> Stand; victim damage 18.
- Whiff: SpecialU -> Landing -> Stand; victim damage 0. This grounded trajectory lands
  before the airborne whiff timeout. Existing airborne command-grab tests cover Helpless.

Both assert ascent, full checksum replay and EOF. Browser stepping also verifies restore
and shows the explosion's 18% damage. The optional fire texture is absent in this checkout;
the renderer now checks resource existence before using its existing procedural fallback,
avoiding repeated load errors. Exact PM startup/catch/launch/momentum remain unverified.
Published through 5e0eacd. Production catch/whiff replay, restore and EOF checks pass;
two-peer Dive under injected delay/loss also passes confirmed-state and reconnect checks.
Exact commands, hashes and receipts are in docs/2_next.md.

## Remaining Falcon special slots: implementation inventory

Inspected chars/falcon.rs, chars/kneeman.rs and moves/special.rs after online cell receipt
99f5ee1. Falcon copies KneeMan's CharSpec and replaces only specials[2].

| Slot | Current definition | Authored startup / active / recovery | Mechanic gap |
| --- | --- | --- | --- |
| Neutral | PUNCH / Punch | 14 / 4 / 26 | PM turnaround count, startup resets and damage increments are not represented by the existing one-shot b_reversed flag |
| Side | LUNGE / Lunge | 8 / 6 / 22 | No Raptor Boost contact-triggered uppercut or separate ground/air ending phases |
| Up | FALCON_DIVE / DiveGrab | 10 / 26 / 20 | Published catch/whiff/recovery tests; exact PM data still unverified |
| Down | DROP / Fall | 8 / 10 / 18 | One impulse for ground/air, no kick-specific landing phase or aerial-jump restoration |

These are current Game3 values, not PM frame data. Generic Lunge clears ground support
at its launch frame. Fall applies the same (220, 700) facing-relative impulse whether
started grounded or airborne. run_special has no air_jumps assignment; normal landing
or ledge refresh elsewhere cannot establish an airborne kick refresh.

Reference recheck: [PMUnofficial Captain Falcon](https://pmunofficial.com/en/characters/captain-falcon/)
labels its public version 3.6+mf and explicitly describes aerial Falcon Kick refreshing the
second jump, plus side-special/Kick B-reverses. This supports a behavior target, without
authenticating an original 3.6 binary or specifying the jump-reset frame.
[Melee decomp SpecialLw](https://github.com/doldecomp/melee/blob/master/src/melee/ft/chara/ftCaptain/ftCa_SpecialLw.c)
was read as a separate Melee reference: distinct ground/air entry, motion-end and collision
callbacks select ending states. Air ending returns through ftCo_Fall_Enter. Animation command
variables and generic callbacks participate; this file alone does not identify jump-reset
timing. The URL is a moving branch, not a pinned PM receipt.

Next bounded move: Falcon Kick. Before implementation, trace the Melee jump-count write and
animation-command boundary or acquire a PM execution receipt. Then define a Game3-authored
sequence that consumes the air jump, presses down-special away from floors, completes the
move and presses jump again. Cover interruption, ground start, landing during travel,
left/right direction and full-state replay. Keep inherited KneeMan Fall behavior isolated
from Falcon-specific changes. Exact PM impulse/frame/hitbox/landing data remain unresolved;
the recovered pm-falcon-kit.md's mixed Melee/PM numbers are not sufficient to claim parity.

### Published completion-time kick checkpoint

The missing Melee write is in [ftCommon_8007D5D4](https://github.com/doldecomp/melee/blob/master/src/melee/ft/ftcommon.c):
it sets x1968_jumpsUsed to 1. SpecialAirLw_Anim calls it when the travel animation finishes,
before switching to SpecialAirLwEndAir. Thus the reference refresh is at entry to the ending
phase, not necessarily when the fighter becomes actionable. These moving-branch sources are
Melee evidence; they do not establish exact original PM 3.6 timing.

Game3 now appends SpecialKind::FallRefreshJump and selects it only through Falcon's down-slot
loadout. At airborne completion it restores the configured max_air_jumps. KneeMan and Lucas
retain Fall. No Fighter fields or Tune fields were added; previous enum discriminants remain
in order. An old decoder cannot understand the appended variant, so mixed-build Tune/startup
compatibility must be checked before publication.

This is a partial kick port: DROP's existing motion/hit data remain, and Game3 refreshes at
completion because it has no separate kick-ending phase yet. Native semantic inputs spend
an air jump at index 0, down-special at 3, and attempt another jump at 45. Falcon restores and
uses the jump; KneeMan does not. Both facings replay 60 ticks with a mid-move snapshot restore.
The new test failed with 0 remaining jumps where 1 was expected before the implementation.
Ground/air trajectories, ending/landing hitboxes, interruption and exact PM frames still need
implementation/acceptance. Do not mark full Falcon Kick or PM parity complete.

### Recovery-entry kick checkpoint

Falcon's loadout now selects appended `FallRefreshOnRecovery`. Its ending interval uses
the existing `AttackData` recovery range: `[active_end(), total())`. On the first ending
tick, an airborne fighter restores `max_air_jumps` once, while remaining in SpecialD until
the interval ends. Grounded entry does not refresh. The current authored boundary is frame
18 within a 36-frame move; these values come from Game3's DROP data, not decoded PM scripts.
No Fighter fields or timers were added. Native tests cover the locked recovery jump press,
snapshots before/after entry, actual hits before/after refresh, and non-repeating entry effects.

The previously published `FallRefreshJump` discriminant 5 keeps its completion-time behavior;
the new variant is appended at 6. Old host Tune data remains executable by the new runtime.
An old guest cannot decode a new host's kind and rejects startup in the mixed-build browser
test. Matching-new and old-host/new-guest loss/reconnect gates also pass; receipts are in
2_next.md. Published through bc50afd; production recovery/loss/reconnect acceptance passes
548 initial and 180 resumed confirmed frames. Ground/air trajectories, dedicated landing
behavior, wall interaction and exact PM animation timing still require work.

### Ground/air travel checkpoint

Appended `Kick { ground_speed }` selects a horizontal grounded launch and an airborne
`(facing * move_x, move_y)` launch. Authored default speeds are 900 px/s for each component;
startup/active/recovery remain 8/10/18. During the active interval, normal gravity/drag/drift
are suspended. The snapshot's velocity preserves launch direction across loss of support;
existing contact resolution and hitlag still apply. Recovery resumes normal physics and
restores airborne jumps once. No new per-fighter fields or timers were introduced.

The ground speed is editable beside the existing special-move velocity controls. This section
targets player 1's shared character-kit row and permits edits only offline. Other Feel groups
retain their existing flat-row behavior. Native
tests cover non-default data, both facings, serialized velocity across an edge, ground/air
travel, recovery/interruption, landing and the left-edge path onto the existing ship.
The recovery fixture moved to y=-250 to keep the inward path above the sails without crossing
the upper blast boundary. Earlier fixture probes at y=-600 crossed that boundary and supply
no recovery evidence. Ground interior starts at x=600 to allow the longer drive.

442 game + 76 shell tests pass. The movement export passes the two-peer recovery/loss/reconnect
gate. The corrected editor's local browser readback retains ground speed 500 after a drag
from 900 and section close/reopen, with air speed unchanged. Published through c0d831a after
mixed-build acceptance. Production online guard rendering and corrected saved-frame agreement
pass across all 61 recovery-sequence frames and through reconnect; 2_next.md records the failed
guest-prediction observation and the subsequent passing gate separately.
Previously published kinds keep their behavior; the new enum payload requires mixed-build
startup acceptance. Speeds and phase timing remain authored Game3 values. Dedicated landing,
wall response and reference-authenticated PM motion/hit data remain incomplete.

### Special landing-recovery checkpoint

Falcon Kick opts into appended `LandCancel::SpecialRecovery`. Ground contact retains the
special slot/state and starts its full existing recovery interval, closing the travel hitbox.
Contact during recovery restarts that interval. Current authored duration is 18 ticks; this
does not include a separate landing hitbox or decoded PM landing-animation duration.

`land_transition(&Tune, CharState)` now returns `(CharState, Option<i64>)`. The optional clock
override is returned through collision and applied after the ordinary state-clock update,
including when landing coincides with entering the special. No stored timer or Fighter field
was added. Ordinary aerials retain normal landing behavior if assigned this specials-only rule.
Published Continue/ResetToLanding discriminants and behavior remain intact; an old decoder
must reject startup data carrying the appended rule. Mixed-build browser tests confirm old-client
rejection with offline recovery and new-client acceptance of the old host's Tune. Published through
32d2dd3; production landing passes all 61 corrected sequence-frame comparisons and reconnect.
Exact commands, hashes and fault-injection receipts are in docs/2_next.md.

Tests cover entry-tick, startup, active travel and recovery contact, full recovery duration,
closed hit windows, snapshot reload, all four ordinary special slots and old discriminants.
Dedicated landing hitboxes, wall response and exact PM timing remain open.

### Kick wall-contact baseline

`falcon_kick_wall_contact_blocks_travel_and_replays` covers both facings and ground/air
starts against drawn vertical walls. Each 60-tick input sequence crosses into the wall
during travel, checks no ECB penetration, zero horizontal velocity and refreshed jumps,
then reloads a serialized contact snapshot and compares every subsequent full-state checksum.
Current contact retains SpecialD, advances its clock normally and leaves its travel hitbox
active. This test records existing collision behavior, not a completed Kick wall-ending phase.

Read-only Melee checkout `cca1beeab039b1a5e8dfe581de7e2e8fb8f0aeef`,
`src/melee/ft/kinds/ftCaptain/ftcaptainspeciallw.c:310`:
`ftCa_SpecialLw_Coll` checks `cmd_vars[0]` plus the facing-selected wall flag, clears flags,
converts to air and enters `ftCa_MS_SpecialHiThrow1`. Its animation callback at line 210
enters Fall when animation ends; its physics callback at line 304 delegates to
`ft_80085134`. The aerial-start collision callback at line 379 only invokes the landing
helper. These callbacks alone do not establish the script gate interval, wall-ending
velocity or duration, or Project M behavior.

Reproducing this Melee transition requires distinguishing ground-launched travel that has left a ledge
from an air-launched Kick, preserve that distinction in snapshots, close the travel hitbox
on the gated transition, and expose ending motion/timing through move data. Current Kick
stores launch velocity but no explicit launch provenance. Do not infer provenance from
velocity equality: move velocities are configurable and collision can modify them.

## Remaining source limits

Recovered `pm-falcon-kit.md` asserts that missing PM changelog entries make Melee values
PM ground truth. That inference is unverified and must not supply acceptance expectations.
Current `src/v1/chars/falcon.rs` inherits KneeMan's kit and replaces special slots 2 and 3
with Falcon Dive and Kick. The generic tuning still labels jumpsquat as universal three frames;
the recovered document claims Falcon four frames. Character timing needs direct data evidence.
No original release PAC attributes, Falcon hitbox scripts or complete PM runtime traces have
been validated here. Original 3.6 artifact authentication and per-character extraction remain open.
