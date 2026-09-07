# Falcon generated-script reference

Rukai Data's PM3.6 pages were retrieved after the wall-contact baseline in `3_pm_baseline.md`.
These are generated reference outputs labeled PM3.6, not authenticated original-release PACs
or a recording of reference gameplay. No downloaded character assets enter Game3.

The author's [generator](https://github.com/rukai/rukaidata) uses
[brawllib_rs](https://github.com/rukai/brawllib_rs) to read fighter files.
The [writeup](https://github.com/rukai/rukaidata/blob/main/docs/writeup.md) describes separate
script/constants, costume skeleton and animation inputs. The generator also accepts a codeset.

## Retrieved facts

| Subaction | Generated data | Main-script observations |
| --- | --- | --- |
| [SpecialLw](https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/SpecialLw.html) | Index 0x1da; active 15–33; IASA absent | Async wait 14; three hitboxes, damage 15, BKB 60, KBG 70, angle 361. Wait 1 enables SpecialsMovement. Wait 2 changes damage to 12, KBG 60, angle 60. Wait 8 changes damage to 9, KBG 50, angle 75. Wait 8 deletes hitboxes. |
| [SpecialAirLw](https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/SpecialAirLw.html) | Index 0x1dc; active summary 16–30; IASA absent | Async wait 15 enables SpecialsMovement and creates two hitboxes. Damage/KBG progress 15/70, 13/65, 11/60; BKB 40 and angle 361. Generated detailed groups are 16–18, 19–26, 27–29, so the summary endpoint needs checking. |
| [SpecialAirLwEnd](https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/SpecialAirLwEnd.html) | Index 0x1df; active frame 1; IASA absent | Three grounded-only hitboxes: damage 10, BKB 65, KBG 35, angle 80, radius 5; bone 0 offsets (0,4,8.5), (0,4,-8.5), (0,4,0). StayOn edge behavior; enables an action-transition flag. Wait 2 deletes hitboxes; waits 1 then 20 disable the flag. |
| [SpecialLwWall](https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/SpecialLwWall.html) | Index 0x1e0; IASA 41 | Async wait 40, SetAirGround(0), allow interrupts. Main script contains no hitbox creation or explicit velocity. |

All frame labels above retain the site's convention. Script wait operands are animation-time
values, not yet accepted as fixed Game3 tick counts. Angle 361 also requires its special
knockback interpretation; it must not become a literal degree angle in Game3.

## Reproduction and hashes

The HTTP bodies returned gzip bytes even without requesting compression. Local downloads:

| Page | Temporary response | SHA-256 of decompressed HTML |
| --- | --- | --- |
| SpecialLw | /private/tmp/game3-pm-falcon-speciallw.html | adf76a90d8ee9a30545c130e48d4280dfc36888f97af976daeff6740d359427d |
| SpecialAirLw | /private/tmp/game3-pm-falcon-air.gz | a22eec44b2e8ad12cabe8c327088b0548e20807ac6e3f0af9e65a6da4195e572 |
| SpecialAirLwEnd | /private/tmp/game3-pm-falcon-airend.gz | 2cc54f653bd10add38b2511de3b99cdc14fa38bdb0c583e04fd0748feb2147dd |
| SpecialLwWall | /private/tmp/game3-pm-falcon-wall.gz | 07ddeb0802d32baed7372657b6cd2eab632f0b69c3adb07932dbb4c70ca5fe79 |

Example read-only inspection of the downloaded response:

```sh
gzip -dc /private/tmp/game3-pm-falcon-airend.gz | shasum -a 256
gzip -dc /private/tmp/game3-pm-falcon-airend.gz |
  sed -n '/<h2>Stats/,/<h3 id="script-gfx"/p' |
  sed -E 's/<[^>]*>/ /g'
```

## Timer interpretation and implementation boundary

Read-only parser checkout: `/private/tmp/game3-brawllib-reference`, revision
`e8dc83313ebc3da31288c076097e80aec47324b2`. In `src/script_runner.rs`, constructor runs the
initial script at index 0; `step` increments index by the frame-speed modifier and then runs
the scripts. SyncWait schedules current index plus its operand; AsyncWait schedules an
absolute index. WiiRD subaction frame-speed modifiers can alter that progression.
`src/high_level_fighter.rs` samples the resulting frame before advancing the runner.
This checkout is not proven to be the revision that generated the retrieved pages.

The landing script's wait 2 versus displayed single active frame remains unresolved;
do not choose a duration by reading the operand alone. The current parser also operates at
subaction level, so these pages do not prove the wall-trigger guard or action-level routing.

Game3 now supplies grounded-only landing shapes through SpecialMove.landing. Combat values
are damage 10, angle 80, BKB 65 and KBG 35; three spatial shapes share hit ID 0. Geometry uses
an authored 6 pixels per reference unit, radius 30 and offsets (+/-51,-24) and (0,-24).
One active tick plus 18 recovery ticks is authored timing. Native tests verify one grounded
hit, overlapping airborne rejection, fresh contact cooldowns and serialized replay.
Remaining evidence: effective duration, bone/world-to-Game3 scale, landing recovery,
wall-entry guard and motion. Preserve ground-launch provenance across a ledge and snapshot
reload when implementing the wall route. Browser/publication status is in docs/2_next.md.

## Ground/air travel rows

The next Game3 checkpoint selects SpecialMove.air_hit from a serialized special-entry flag.
Ground damage progresses 15/12/9 with BKB 60 and KBG 70/60/50; air progresses 15/13/11 with
BKB 40 and KBG 70/65/60. Shapes share hit ID 0 across phases. These damage/KB values come
from the retrieved ground/air scripts above. Game3 retains startup 8, ten travel ticks split
3/4/3, recovery 18 and its existing spatial shape. Those durations and geometry are authored.
The working angle-361 resolver now uses victim contact and KB with the source formula below;
ground middle/late angles remain 60/75. The air branch retains 45 degrees. PM-equivalent
runtime behavior remains unverified; publication status is tracked in docs/2_next.md.
Special entry context survives leaving a ledge and snapshot restore; current groundedness
does not reselect the travel row. Wall guards and animation-driven recoil remain unported.

## Angle-361 source checkpoint

Read-only Melee revision `cca1beeab039b1a5e8dfe581de7e2e8fb8f0aeef`:
`src/melee/ft/kinds/ftCommon/ftCo_Damage.c:80`, `ftCo_Damage_CalcAngle`.
Ordinary angles convert directly to radians. Sentinel 361 reads the **victim's**
ground/air state. Air returns common-data `x144_radians`; ground below `x14C` returns
zero; otherwise degrees are `min(x148, x148 * ((kb - x14C)/(x150 - x14C)) + 1)`.
At line 328 the caller supplies applied knockback, before launch-vector speed scaling.
This source establishes the algorithm, not the numeric contents of the common-data fields.

The local [Project-M-CC codes](https://github.com/Project-M-CC/Project-M-CC/tree/6e63ffa920d45e9d0236edbec4bf43057e7b3e2d/%5BDev%20Resources%5D)
at revision `6e63ffa920d45e9d0236edbec4bf43057e7b3e2d` contain:

| Write | IEEE-754 float |
| --- | --- |
| `04B87ABC 42300000` | 44 |
| `04B87AC0 42000000` | 32 |
| `04B87AC4 42006666` | 32.099998474121094 |

Identical lines occur in `codes-3_6.txt:2423`, `codes-3_6-wifi.txt:2284`, and
`codes-3_61.txt:2659`. The latter labels the patch “Melee 361 Angle [Magus]”.
That file is UTF-16LE; decode before line-oriented inspection. These are repository
code-list receipts, not verification that a specific running PM executable applied them.
The mapping of these three values to cap/lower/upper parameters is an inference from the
named patch and Melee function. PM engine address consumers and the air-angle constant
still need independent source confirmation before claiming full PM equivalence.

Implementation boundary in Game3: `combat::strike` computes KB units before multiplying
by `Tune.kb_speed` and `knockback_mult`. `Aim::resolve` currently lacks KB and victim
contact state. `Fighter::absorb` clears support during interruption, so capture groundedness
before that call. Use victim contact state, independently of the attacker's serialized
`special_started_air` that selects the attack row. Reuse the existing grounded target test
(support present, state not airborne, excluding ledge hold/climb) rather than stale support alone.
Shared strike receivers include Fighter, Item and InkPath; define non-fighter sentinel
behavior explicitly when extending this path. Ordinary angles, radial and carry aims must
retain their current results. Keep shared receiver code free of Falcon move selection.

Required acceptance before switching Falcon rows to 361: ordinary-angle controls; air and
ground victim branches; below/at/inside/above 32..32.1 KB; facing signs; pre-speed-scale KB;
stale support; shield/invulnerability; non-fighter receivers; serialized replay and two-peer
corrected hashes. Under the inferred Melee constants, exact KB 32 yields 1 degree, 32.05
yields 23, and 32.1 reaches the 44-degree cap. Preserve the source's +1 term. PowerPC
instruction rounding and exact PM runtime equivalence are not established by these formulas.
Changing launch semantics also requires a startup compatibility-version bump so old and
new peers cannot start an apparently compatible simulation.

Implementation checkpoint: `combat::strike` resolves sentinel Angle aims after KB scaling
by the Swing and before pixel-speed scaling affects direction. `launch_grounded` defaults
false for Item/InkPath; Fighter overrides it using current support and state. This is explicit
Game3 non-fighter policy. Ordinary Angle, Radial and Carry aims keep their previous path.
Falcon's strong ground and all air rows use 361; the editor range includes it so opening
controls cannot clamp the sentinel to 180. Startup compatibility is now version 5.
Native gate passes 457 game + 78 shell tests, including numeric boundaries, both facing signs,
stale air/ledge support, serialized fighter restore, guard behavior and Item/InkPath response.
Local editor/replay and mixed 5/4 rejection pass. Published version 5 passes grounded-victim
early trajectory checks against native replay and a counterfactual fixed-45 path, plus two-peer
message-loss/reconnect acceptance. Exact receipts are in docs/2_next.md. This verifies Game3's
implemented behavior; PM engine address mapping and exact original-runtime equivalence remain open.
