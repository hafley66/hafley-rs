//! Roster row 2: Lucas, the "fast-Lucas" pass (plans/swordsman-lucas.md row 7, PARTIAL). Everything
//! here is expressible in TODAY's data model: a floaty, strong-air-game, average-to-slow-ground
//! `CharData`; 15 `AttackData` normals built around a stick weapon (several boxes reach BEYOND the
//! `DUMMY_R` (48px) hurtbox circle -- the swordsman-precedent "disjoint" shape, `transcendent: true`
//! so the stick never clanks); a big stretched grab (`grab_range` alone, see below); and specials
//! STUBBED to existing `SpecialKind`s until the PK verbs land.
//!
//! Two identity traits authored as pure data, per plans/swordsman-lucas.md's Lucas note:
//!   * disjoints: `fair`/`uair`/`dtilt`/`ftilt`/`utilt`/`fsmash`(tip)/`usmash`/`dsmash`/
//!     `ledge_attack`/`getup_attack` all carry at least one box whose near edge
//!     (`off.length() - r`) clears `DUMMY_R` (48.0) -- the stick reaches past his own body.
//!     `fsmash` is the plan's suggested two-box smash: a close blunt `hilt` (id 1, NOT disjoint,
//!     NOT transcendent) plus a far `tip` (id 0, disjoint, `transcendent: true`, the kill sweetspot).
//!   * tether grab: there is no `Grab`-role `Hitbox` in this codebase today -- `resolve_grab`
//!     (moves/throw.rs) catches off a scalar `CharSpec.grab_range` + a fixed slop constant
//!     (`GRAB_CATCH_R`), never a `Hitbox` row. The tether is authored as pure data on that existing
//!     scalar (`grab_range: 230.0`, well past KNEEMAN's `100.0`) rather than inventing a new
//!     Hitbox "role" field -- same mechanism KNEEMAN's grab uses, just a bigger number.
//!
//! No armor windows: `GuardResponse`/`Guard` (plans/swordsman-lucas.md row 5) has not landed --
//! `AttackData` has no `guard` field to set. Absorb (down-B) and PK Thunder steering are likewise
//! unbuilt (rows 3b/6); see the `// STUB` comment on each special below.

use crate::v1::{
    AttackData, CharData, CharSpec, Hitbox, SpecialKind, SpecialMove, ThrowData, Vector2,
};

/// Lucas's `CharSpec`. Row 2 of the sim roster (`chars::roster()`); see `chars/mod.rs` for the art
/// roster alignment note (`assets/roster.json` lists Lucas at a different index -- shell art is a
/// separate list from the sim `Tune.roster`, cosmetic only, see `rust-sim/shell/src/roster.rs`).
pub const fn spec() -> CharSpec {
    CharSpec {
        phys: CharData {
            // floaty faller: low gravity/max_fall/fastfall next to KNEEMAN's 0.17/2.9/4.2.
            gravity: 0.11,
            max_fall: 2.4,
            fastfall: 3.4,
            // average-to-slow ground speed: under KNEEMAN's 0.85/1.9/2.34.
            walk_max: 0.75,
            dash_init: 1.6,
            run_max: 2.05,
            ground_accel: 0.11,
            ground_friction: 0.20,
            // Jump velocities are LOWER than KNEEMAN's, but the low gravity (0.11 vs 0.17) makes
            // each apex TALLER: apex is v^2/(2*gravity). fullhop apex 3.05^2/0.11 = 84.6 vs
            // KNEEMAN's 3.68^2/0.17 = 79.7 -- floatier by feel, same ballpark height.
            fullhop_v: 3.05,
            shorthop_v: 1.50,
            // Strong air game via air CONTROL (airjump_h/air_speed/air_accel past KNEEMAN's
            // 1.40/1.60/0.22), NOT a big pop. airjump_v is deliberately BELOW fullhop_v (PM double
            // jumps don't exceed the full hop): DJ apex 2.75^2/0.11 = 68.8, under his own fullhop
            // 84.6 and just over KNEEMAN's DJ 64.1. The old 3.60 gave a 117.8 DJ apex on 0.11 gravity
            // -- a standing fullhop+DJ cleared 232 units and flew off the top blast zone ("double
            // jump himself to death"). New standing ceiling 84.6+68.8 = 153, ~1.07x KNEEMAN's 143.8.
            airjump_v: 2.75,
            airjump_h: 1.70,
            jump_h_init: 1.00,
            jump_h_max: 2.30,
            air_speed: 1.95,
            air_accel: 0.30,
            air_friction: 0.012,
            momentum_carry: 1.0,
            max_air_jumps: 1,
            max_air_dodges: 1,
            roll_speed: 1.7,
            airdodge_speed: 3.0,
            airdodge_drag: 0.15,
            ledgejump_v: 2.30, // 2.30^2/0.11 = 48.1 apex; the old 2.60 gave 61.5, another top-blast risk
            jumpsquat: 4,
            landing_lag: 5,
            dash_window: 12,
            pivot_frames: 1,
            dash_turn_accel: 0.45,
            dashstop_friction: 0.26,
            spotdodge_frames: 22,
            roll_frames: 23,
            airdodge_frames: 28,
            ledge_intang: 30,
            climb_frames: 25,
            buffer_frames: 12,
        },
        // quick bare-fist flurry: two close hits, no disjoint (contrast with the stick moves below).
        jab: AttackData::new(
            4,
            10,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 4,
                    len: 2,
                    off: Vector2::new(36.0, -60.0),
                    r: 28.0,
                    damage: 2.0,
                    angle: 70.0,
                    bkb: 6.0,
                    kbg: 0.0,
                    set_kb: 5.0,
                    transcendent: false,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 8,
                    len: 3,
                    off: Vector2::new(40.0, -58.0),
                    r: 30.0,
                    damage: 4.0,
                    angle: 75.0,
                    bkb: 26.0,
                    kbg: 36.0,
                    set_kb: 0.0,
                    transcendent: false,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // neutral aerial: a bare kick sex-kick, same shape as KNEEMAN's but softer numbers.
        nair: AttackData::new(
            5,
            14,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 5,
                    len: 5,
                    off: Vector2::new(24.0, -58.0),
                    r: 48.0,
                    damage: 7.0,
                    angle: 40.0,
                    bkb: 20.0,
                    kbg: 40.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 10,
                    len: 6,
                    off: Vector2::new(24.0, -58.0),
                    r: 48.0,
                    damage: 4.0,
                    angle: 25.0,
                    bkb: 12.0,
                    kbg: 22.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // forward aerial: a stick swing. id0 tip is DISJOINT (near edge 86-30=56 > DUMMY_R 48);
        // id1 is a close bare-arm sourspot tail (near edge 12, not disjoint).
        fair: AttackData::new(
            7,
            16,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 7,
                    len: 3,
                    off: Vector2::new(86.0, -54.0),
                    r: 30.0,
                    damage: 15.0,
                    angle: 34.0,
                    bkb: 28.0,
                    kbg: 84.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 10,
                    len: 9,
                    off: Vector2::new(48.0, -54.0),
                    r: 36.0,
                    damage: 6.0,
                    angle: 44.0,
                    bkb: 12.0,
                    kbg: 20.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // back aerial: bare-foot kick, close (not disjoint).
        bair: AttackData::one(
            6,
            5,
            13,
            Hitbox {
                damage: 11.0,
                off: Vector2::new(-52.0, -58.0),
                r: 42.0,
                angle: 35.0,
                bkb: 22.0,
                kbg: 58.0,
                transcendent: true,
                ..Hitbox::NONE
            },
        ),
        // up aerial: overhead stick swing sweeping front-up to back-up. Both boxes DISJOINT
        // (near edges ~93 / ~99, both past DUMMY_R 48).
        uair: AttackData::new(
            6,
            12,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 6,
                    len: 4,
                    off: Vector2::new(14.0, -132.0),
                    r: 40.0,
                    damage: 11.0,
                    angle: 78.0,
                    bkb: 22.0,
                    kbg: 50.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 10,
                    len: 5,
                    off: Vector2::new(-14.0, -136.0),
                    r: 38.0,
                    damage: 7.0,
                    angle: 85.0,
                    bkb: 16.0,
                    kbg: 38.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // down aerial: a foot stomp/spike then a sourspot tail, bare-limb (not disjoint).
        dair: AttackData::new(
            8,
            10,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 8,
                    len: 3,
                    off: Vector2::new(16.0, 20.0),
                    r: 50.0,
                    damage: 12.0,
                    angle: -78.0,
                    bkb: 18.0,
                    kbg: 40.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 11,
                    len: 6,
                    off: Vector2::new(20.0, 50.0),
                    r: 46.0,
                    damage: 6.0,
                    angle: 50.0,
                    bkb: 10.0,
                    kbg: 22.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // down tilt: a low stick poke, reaching far along the ground. Both boxes DISJOINT
        // (near edge 90-26=64 > DUMMY_R 48).
        dtilt: AttackData::new(
            5,
            11,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 5,
                    len: 3,
                    off: Vector2::new(90.0, 2.0),
                    r: 26.0,
                    damage: 7.0,
                    angle: 78.0,
                    bkb: 16.0,
                    kbg: 34.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 9,
                    len: 6,
                    off: Vector2::new(90.0, 2.0),
                    r: 26.0,
                    damage: 4.0,
                    angle: 85.0,
                    bkb: 10.0,
                    kbg: 20.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // forward tilt: a mid-range stick sweep. DISJOINT (near edge 96-34=62 > 48).
        ftilt: AttackData::one(
            6,
            4,
            15,
            Hitbox {
                damage: 10.0,
                off: Vector2::new(96.0, -46.0),
                r: 34.0,
                angle: 35.0,
                bkb: 22.0,
                kbg: 44.0,
                transcendent: true,
                ..Hitbox::NONE
            },
        ),
        // up tilt: overhead stick swing. DISJOINT (near edge ~140.4-40=100.4 > 48).
        utilt: AttackData::one(
            7,
            6,
            17,
            Hitbox {
                damage: 9.0,
                off: Vector2::new(10.0, -140.0),
                r: 40.0,
                angle: 85.0,
                bkb: 24.0,
                kbg: 48.0,
                transcendent: true,
                ..Hitbox::NONE
            },
        ),
        // forward smash: the plan's suggested two-box smash. id0 `tip` is the far, DISJOINT kill
        // sweetspot (near edge 120-30=90 > 48, transcendent -- the stick reach). id1 `hilt` is the
        // close, NOT-disjoint, NOT-transcendent blunt sourspot (near edge 50-34=16).
        fsmash: AttackData::new(
            15,
            26,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 15,
                    len: 4,
                    off: Vector2::new(120.0, -50.0),
                    r: 30.0,
                    damage: 19.0,
                    angle: 38.0,
                    bkb: 70.0,
                    kbg: 190.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 15,
                    len: 4,
                    off: Vector2::new(50.0, -50.0),
                    r: 34.0,
                    damage: 10.0,
                    angle: 35.0,
                    bkb: 30.0,
                    kbg: 80.0,
                    set_kb: 0.0,
                    transcendent: false,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // up smash: overhead double stick swing. DISJOINT (near edge ~150.1-42=108.1 > 48).
        usmash: AttackData::one(
            10,
            6,
            28,
            Hitbox {
                damage: 16.0,
                off: Vector2::new(6.0, -150.0),
                r: 42.0,
                angle: 86.0,
                bkb: 58.0,
                kbg: 178.0,
                transcendent: true,
                ..Hitbox::NONE
            },
        ),
        // down smash: front/back stick sweep, mirrored. Both boxes DISJOINT (near edge 84-30=54 > 48).
        dsmash: AttackData::new(
            9,
            24,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 9,
                    len: 3,
                    off: Vector2::new(84.0, -8.0),
                    r: 30.0,
                    damage: 14.0,
                    angle: 24.0,
                    bkb: 54.0,
                    kbg: 148.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 14,
                    len: 3,
                    off: Vector2::new(-84.0, -8.0),
                    r: 30.0,
                    damage: 14.0,
                    angle: 156.0,
                    bkb: 54.0,
                    kbg: 148.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        // dash attack: a shoulder/stick body-check, not disjoint (near edge 80-48=32).
        dash_attack: AttackData::one(
            9,
            34,
            7,
            Hitbox {
                damage: 9.0,
                off: Vector2::new(80.0, -52.0),
                r: 48.0,
                angle: 28.0,
                bkb: 22.0,
                kbg: 36.0,
                ..Hitbox::NONE
            },
        ),
        // ledge getup attack: a stick sweep clearing the lip. DISJOINT (near edge 98-32=66 > 48).
        ledge_attack: AttackData::one(
            15,
            4,
            19,
            Hitbox {
                damage: 8.5,
                off: Vector2::new(98.0, -36.0),
                r: 32.0,
                angle: 40.0,
                bkb: 22.0,
                kbg: 38.0,
                transcendent: true,
                ..Hitbox::NONE
            },
        ),
        // knockdown getup attack: front/back stick sweep, mirrored. Both DISJOINT (near edge 54 > 48).
        getup_attack: AttackData::new(
            13,
            15,
            [
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 0,
                    start: 13,
                    len: 3,
                    off: Vector2::new(84.0, -40.0),
                    r: 30.0,
                    damage: 7.5,
                    angle: 45.0,
                    bkb: 20.0,
                    kbg: 32.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox {
                    targets: crate::v1::HitTargets::Both,
                    id: 1,
                    start: 17,
                    len: 3,
                    off: Vector2::new(-84.0, -40.0),
                    r: 30.0,
                    damage: 7.5,
                    angle: 135.0,
                    bkb: 20.0,
                    kbg: 32.0,
                    set_kb: 0.0,
                    transcendent: true,
                    refresh: 0,
                },
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            2,
        ),
        specials: [
            // neutral-B: PK Freeze has no verb yet (only Missile/Thunder/Absorb are planned rows) --
            // placeholder Punch, a short-range zap-punch.
            // STUB: -> a freeze/ice verb, if one ever lands; no row currently plans it.
            SpecialMove {
                kind: SpecialKind::Punch,
                hit: AttackData::one(
                    10,
                    4,
                    20,
                    Hitbox {
                        damage: 14.0,
                        off: Vector2::new(50.0, -56.0),
                        r: 40.0,
                        angle: 40.0,
                        bkb: 36.0,
                        kbg: 110.0,
                        ..Hitbox::NONE
                    },
                ),
                move_x: 120.0,
                move_y: -20.0,
                no_gravity: false,
                hang_vel: 0.0,
                landing: None,
                air_hit: None,
            },
            // side-B: PK Fire. STUB: -> Missile (Act::SpecialFire + a projectile) when row 3 lands.
            SpecialMove {
                kind: SpecialKind::Lunge,
                hit: AttackData::one(
                    9,
                    5,
                    20,
                    Hitbox {
                        damage: 10.0,
                        off: Vector2::new(58.0, -54.0),
                        r: 38.0,
                        angle: 50.0,
                        bkb: 34.0,
                        kbg: 96.0,
                        ..Hitbox::NONE
                    },
                ),
                move_x: 760.0,
                move_y: -80.0,
                no_gravity: false,
                hang_vel: 0.0,
                landing: None,
                air_hit: None,
            },
            // up-B: PK Thunder 2. STUB: -> Thunder (steered bolt + self-hit launch) when row 3b lands.
            // `no_gravity: true` is the Ness/Lucas floaty-recovery hook `SpecialMove` already
            // documents (moves/special.rs) -- holds the burst instead of arcing, so the real
            // recovery is at least honestly floaty even stubbed to a plain `Rise`.
            SpecialMove {
                kind: SpecialKind::Rise,
                hit: AttackData::one(
                    6,
                    8,
                    20,
                    Hitbox {
                        damage: 6.0,
                        off: Vector2::new(18.0, -92.0),
                        r: 42.0,
                        angle: 82.0,
                        bkb: 40.0,
                        kbg: 70.0,
                        ..Hitbox::NONE
                    },
                ),
                move_x: 260.0,
                move_y: -900.0,
                no_gravity: true,
                hang_vel: 0.0,
                landing: None,
                air_hit: None,
            },
            // down-B: PSI Magnet. STUB: -> Absorb (on_proj heal+despawn) when row 6 lands.
            SpecialMove {
                kind: SpecialKind::Fall,
                hit: AttackData::one(
                    10,
                    6,
                    16,
                    Hitbox {
                        damage: 8.0,
                        off: Vector2::new(20.0, 14.0),
                        r: 40.0,
                        angle: -60.0,
                        bkb: 30.0,
                        kbg: 66.0,
                        ..Hitbox::NONE
                    },
                ),
                move_x: 140.0,
                move_y: 460.0,
                no_gravity: false,
                hang_vel: 0.0,
                landing: None,
                air_hit: None,
            },
        ],
        throws: [
            ThrowData {
                damage: 9.0,
                kb_base: 540.0,
                kb_scale: 4.0,
                kb_angle: 46.0,
            },
            ThrowData {
                damage: 11.0,
                kb_base: 640.0,
                kb_scale: 4.4,
                kb_angle: 52.0,
            },
            ThrowData {
                damage: 7.5,
                kb_base: 580.0,
                kb_scale: 4.2,
                kb_angle: 86.0,
            },
            ThrowData {
                damage: 6.5,
                kb_base: 460.0,
                kb_scale: 3.4,
                kb_angle: 70.0,
            },
        ],
        dair_threshold: 0.5,
        fastfall_threshold: 0.6,
        autohop_dmg: 0.85,
        // tether grab: the only mechanism this codebase has for grab reach is the scalar
        // `grab_range` (+ the fixed `GRAB_CATCH_R` slop in moves/throw.rs) -- there is no
        // `Hitbox`-shaped "Grab role" to author against. `grab_range` more than doubles KNEEMAN's
        // 100.0, the "big stretched grab box" as pure data on the field that already exists.
        grab_startup: 9, // slower wind-up than KNEEMAN's 6: a committal stretch, not a quick snatch
        grab_active: 5,
        grab_recovery: 34, // punishable whiff, past KNEEMAN's 28: missing a tether is a bigger opening
        grab_range: 230.0, // KNEEMAN: 100.0 -- the tether
        grab_hold: 140,
        grab_mash: 9,
        pummel_damage: 2.0,
        pummel_bonus: 14,
        weight: 92.0, // moderate: lighter than KNEEMAN's 104
        zone_exempt: false,
        charge_max: 60,
        charge_dmg: 1.4,
        shield_max: crate::v1::SHIELD_MAX,
        shield_regen: 0.1,
        shield_decay: 0.2,
        shieldstun_per_dmg: 0.6,
        shield_push: 24.0,
        shieldbreak_frames: 180,
        walljump_v: 2.4,
        walljump_h: 1.5,
        footstool_v: 2.2,
        footstool_spike: 1.5,
        footstool_stun: 24,
        crawl_speed: 0.4,
        ac_grav_mult: 2.0,
        ac_boost_accel: 0.5,
        ac_boost_max: 3.4,
        ac_qb_speed: 3.2,
        ac_qb_cd: 40,
    }
}
