//! Roster row 0: Knee Man, the reference character every fighter runs today (mirrors
//! `items/` -- one file per character, moved out of tune.rs so "add a character" means
//! "add a file" (plans/swordsman-lucas.md row 1)).

use crate::v1::{AttackData, CharData, CharSpec, SpecialMove, ThrowData};

/// Knee Man's `CharSpec`. Byte-identical to the old `CharSpec::KNEEMAN` const this
/// replaces; `tune.rs` keeps a `CharSpec::KNEEMAN` const that forwards here so every
/// existing call site (tests, `Tune::default`) is untouched.
pub const fn spec() -> CharSpec {
    CharSpec {
        phys: CharData::KNEEMAN,
        jab: AttackData::JAB,
        nair: AttackData::NAIR,
        fair: AttackData::FAIR,
        bair: AttackData::BAIR,
        uair: AttackData::UAIR,
        dair: AttackData::DAIR,
        dtilt: AttackData::DTILT,
        ftilt: AttackData::FTILT,
        utilt: AttackData::UTILT,
        fsmash: AttackData::FSMASH,
        usmash: AttackData::USMASH,
        dsmash: AttackData::DSMASH,
        dash_attack: AttackData::DASH_ATTACK,
        ledge_attack: AttackData::LEDGE_ATTACK,
        getup_attack: AttackData::GETUP_ATTACK,
        specials: [
            SpecialMove::PUNCH,
            SpecialMove::LUNGE,
            SpecialMove::RISE,
            SpecialMove::DROP,
        ],
        throws: [
            ThrowData::FWD,
            ThrowData::BACK,
            ThrowData::UP,
            ThrowData::DOWN,
        ],
        dair_threshold: 0.5,
        fastfall_threshold: 0.6,
        autohop_dmg: 0.85, // Ultimate-ish 15% cut on the easy jump+attack aerial
        grab_startup: 6,
        grab_active: 4,
        grab_recovery: 28, // whiff lag: missing a grab leaves you open
        grab_range: 100.0,
        grab_hold: 140,
        grab_mash: 9,
        pummel_damage: 2.4,
        pummel_bonus: 14,
        weight: 104.0, // Falcon/KneeMan-ish; lighter = flies further (the PM combo weight)
        zone_exempt: false,
        charge_max: 60,
        charge_dmg: 1.4,
        shield_max: crate::v1::SHIELD_MAX, // 60: ~3.5 fsmashes, or ~5s of raw hold
        shield_regen: 0.1,
        shield_decay: 0.2,
        shieldstun_per_dmg: 0.6,
        shield_push: 24.0,
        shieldbreak_frames: 180,
        walljump_v: 2.6, // src; between shorthop (1.8) and fullhop (3.68)
        walljump_h: 1.6,
        footstool_v: 2.2,
        footstool_spike: 1.5,
        footstool_stun: 24,
        crawl_speed: 0.45,   // about half walk
        ac_grav_mult: 2.0,   // falls like a dropped fridge
        ac_boost_accel: 0.5, // out-thrusts the doubled gravity with room to climb
        ac_boost_max: 3.4,   // just past fullhop speed, sustained
        ac_qb_speed: 3.2,
        ac_qb_cd: 40,
    }
}
