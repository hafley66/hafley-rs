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
