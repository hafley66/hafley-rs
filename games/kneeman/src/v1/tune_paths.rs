//! Path-addressing surface over `Tune`, GENERATED from the type graph by
//! `.dl/gen-tune-paths.dl` (elected by the `// sprefa:paths` marker on `struct Tune`).
//!
//! core-rx-refactor.md row 10 (kit S4's sprefa-codegen track over `#[derive(Paths)]`):
//! the same gen-sink + staleness-rail shape as row 9's `core/src/items/registry_gen.rs`.
//!
//! Do NOT hand-edit between the generator's marker pairs -- rerun
//! `just regen-tune-paths` (`dl .dl/gen-tune-paths.dl --root .`; the pre-commit
//! `dl --check` rail `.dl/tune-paths-staleness.dl` fires a diag when this file drifts).
//!
//! Scalar leaves only (f32 / i64 / bool). Non-scalar fields of `Tune` (AttackData,
//! ItemConfig, [SpecialMove; 4], StrokeRegistry, ZoneMode, Roster, ...) are out of
//! scope for the flat generator -- they need the nested type walk (kit S1's "nested
//! support second" step, rust-signals-kit.md S4's SCIP-backed follow-on).

#![allow(clippy::enum_variant_names)]

use crate::v1::Tune;

/// Minimal leaf-value repr: one arm per scalar leaf type. The full `Value` design is
/// kit S0 (out of scope here). f32 blocks a derived `Eq`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TuneValue {
    F32(f32),
    I64(i64),
    Bool(bool),
}

/// Reflective path id: one variant per scalar leaf field of `Tune`, upper-camel.
/// Serde-able plain data (the reflective half of principle 2); rides in actions,
/// the meta sink, and the devtool.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TunePath {
    // sprefa:gen variants
    AcBoostAccel,
    AcBoostMax,
    AcGravMult,
    AcQbCd,
    AcQbSpeed,
    AcSpawnWeight,
    AirAccel,
    AirFriction,
    AirSpeed,
    AirdodgeDrag,
    AirdodgeFrames,
    AirdodgeSpeed,
    AirjumpH,
    AirjumpV,
    AutohopDmg,
    BRevWindow,
    BadgeSpawnWeight,
    BoosterCos,
    BoosterKb,
    BoosterLen,
    BoosterStun,
    BufferFrames,
    ChargeDmg,
    ChargeMax,
    ClankDiff,
    ClimbFrames,
    ClingFrames,
    CoyoteFrames,
    CrawlSpeed,
    DairThreshold,
    DashInit,
    DashTurnAccel,
    DashWindow,
    DashstopFriction,
    DiMaxAngle,
    Fastfall,
    FastfallThreshold,
    FloorBounce,
    FootstoolSpike,
    FootstoolStun,
    FootstoolV,
    FullhopV,
    GetupFrames,
    GrabActive,
    GrabHold,
    GrabMash,
    GrabRange,
    GrabRecovery,
    GrabStartup,
    Gravity,
    GroundAccel,
    GroundFriction,
    HandReachX,
    HandRise,
    InkBudget,
    InkCursorReach,
    InkLaunchSpeed,
    InkSpawnWeight,
    ItemSpawnInterval,
    ItemsOn,
    JumpHInit,
    JumpHMax,
    Jumpsquat,
    KbHitstun,
    KbSpeed,
    KnockbackMult,
    KnockdownFrames,
    LandingLag,
    LedgeCeil,
    LedgeFallEps,
    LedgeGrabR,
    LedgeIntang,
    LedgeLipBite,
    LedgeMinLen,
    LedgeReachDown,
    LedgeReachX,
    LedgejumpV,
    MaxAirDodges,
    MaxAirJumps,
    MaxFall,
    MomentumCarry,
    OneItemAtATime,
    PickupR,
    PickupReach,
    PivotFrames,
    PlatDropWindow,
    PummelBonus,
    PummelDamage,
    ReboundFrames,
    ReboundPush,
    RollFrames,
    RollSpeed,
    RunSpeed,
    SaveScale,
    SaveZeroPct,
    ShieldDecay,
    ShieldMax,
    ShieldPush,
    ShieldRegen,
    ShieldbreakFrames,
    ShieldstunPerDmg,
    ShipThrustAccel,
    ShorthopV,
    SmashWindow,
    SpawnIframes,
    SpotdodgeFrames,
    TechIntang,
    TechWindow,
    TechrollFrames,
    TechrollSpeed,
    TumbleSpeed,
    WalkSpeed,
    WallBounce,
    WalljumpH,
    WalljumpV,
    Weight,
    ZoneExempt,
    // sprefa:end
}

impl TunePath {
    /// Routed write: set this path's field on `t` from a `TuneValue`. A wrong-arm
    /// value (e.g. `Bool` for an `f32` field) is a no-op, not a panic.
    pub fn set(self, t: &mut Tune, v: TuneValue) {
        match self {
            // sprefa:gen set-arms
            TunePath::AcBoostAccel => {
                t.ac_boost_accel = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AcBoostMax => {
                t.ac_boost_max = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AcGravMult => {
                t.ac_grav_mult = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AcQbCd => t.ac_qb_cd = if let TuneValue::I64(x) = v { x } else { return },
            TunePath::AcQbSpeed => {
                t.ac_qb_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AcSpawnWeight => {
                t.ac_spawn_weight = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AirAccel => t.air_accel = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::AirFriction => {
                t.air_friction = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AirSpeed => t.air_speed = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::AirdodgeDrag => {
                t.airdodge_drag = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AirdodgeFrames => {
                t.airdodge_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::AirdodgeSpeed => {
                t.airdodge_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::AirjumpH => t.airjump_h = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::AirjumpV => t.airjump_v = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::AutohopDmg => {
                t.autohop_dmg = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::BRevWindow => {
                t.b_rev_window = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::BadgeSpawnWeight => {
                t.badge_spawn_weight = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::BoosterCos => {
                t.booster_cos = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::BoosterKb => {
                t.booster_kb = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::BoosterLen => {
                t.booster_len = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::BoosterStun => {
                t.booster_stun = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::BufferFrames => {
                t.buffer_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ChargeDmg => {
                t.charge_dmg = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ChargeMax => {
                t.charge_max = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ClankDiff => {
                t.clank_diff = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ClimbFrames => {
                t.climb_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ClingFrames => {
                t.cling_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::CoyoteFrames => {
                t.coyote_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::CrawlSpeed => {
                t.crawl_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::DairThreshold => {
                t.dair_threshold = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::DashInit => t.dash_init = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::DashTurnAccel => {
                t.dash_turn_accel = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::DashWindow => {
                t.dash_window = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::DashstopFriction => {
                t.dashstop_friction = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::DiMaxAngle => {
                t.di_max_angle = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::Fastfall => t.fastfall = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::FastfallThreshold => {
                t.fastfall_threshold = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::FloorBounce => {
                t.floor_bounce = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::FootstoolSpike => {
                t.footstool_spike = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::FootstoolStun => {
                t.footstool_stun = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::FootstoolV => {
                t.footstool_v = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::FullhopV => t.fullhop_v = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::GetupFrames => {
                t.getup_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::GrabActive => {
                t.grab_active = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::GrabHold => t.grab_hold = if let TuneValue::I64(x) = v { x } else { return },
            TunePath::GrabMash => t.grab_mash = if let TuneValue::I64(x) = v { x } else { return },
            TunePath::GrabRange => {
                t.grab_range = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::GrabRecovery => {
                t.grab_recovery = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::GrabStartup => {
                t.grab_startup = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::Gravity => t.gravity = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::GroundAccel => {
                t.ground_accel = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::GroundFriction => {
                t.ground_friction = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::HandReachX => {
                t.hand_reach_x = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::HandRise => t.hand_rise = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::InkBudget => {
                t.ink_budget = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::InkCursorReach => {
                t.ink_cursor_reach = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::InkLaunchSpeed => {
                t.ink_launch_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::InkSpawnWeight => {
                t.ink_spawn_weight = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ItemSpawnInterval => {
                t.item_spawn_interval = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ItemsOn => {
                t.items_on = if let TuneValue::Bool(x) = v {
                    x
                } else {
                    return;
                }
            }
            TunePath::JumpHInit => {
                t.jump_h_init = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::JumpHMax => t.jump_h_max = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::Jumpsquat => t.jumpsquat = if let TuneValue::I64(x) = v { x } else { return },
            TunePath::KbHitstun => {
                t.kb_hitstun = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::KbSpeed => t.kb_speed = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::KnockbackMult => {
                t.knockback_mult = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::KnockdownFrames => {
                t.knockdown_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::LandingLag => {
                t.landing_lag = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::LedgeCeil => {
                t.ledge_ceil = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeFallEps => {
                t.ledge_fall_eps = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeGrabR => {
                t.ledge_grab_r = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeIntang => {
                t.ledge_intang = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::LedgeLipBite => {
                t.ledge_lip_bite = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeMinLen => {
                t.ledge_min_len = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeReachDown => {
                t.ledge_reach_down = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgeReachX => {
                t.ledge_reach_x = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::LedgejumpV => {
                t.ledgejump_v = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::MaxAirDodges => {
                t.max_air_dodges = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::MaxAirJumps => {
                t.max_air_jumps = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::MaxFall => t.max_fall = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::MomentumCarry => {
                t.momentum_carry = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::OneItemAtATime => {
                t.one_item_at_a_time = if let TuneValue::Bool(x) = v {
                    x
                } else {
                    return;
                }
            }
            TunePath::PickupR => t.pickup_r = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::PickupReach => {
                t.pickup_reach = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::PivotFrames => {
                t.pivot_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::PlatDropWindow => {
                t.plat_drop_window = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::PummelBonus => {
                t.pummel_bonus = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::PummelDamage => {
                t.pummel_damage = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ReboundFrames => {
                t.rebound_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ReboundPush => {
                t.rebound_push = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::RollFrames => {
                t.roll_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::RollSpeed => {
                t.roll_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::RunSpeed => t.run_speed = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::SaveScale => {
                t.save_scale = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::SaveZeroPct => {
                t.save_zero_pct = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShieldDecay => {
                t.shield_decay = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShieldMax => {
                t.shield_max = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShieldPush => {
                t.shield_push = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShieldRegen => {
                t.shield_regen = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShieldbreakFrames => {
                t.shieldbreak_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::ShieldstunPerDmg => {
                t.shieldstun_per_dmg = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShipThrustAccel => {
                t.ship_thrust_accel = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::ShorthopV => {
                t.shorthop_v = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::SmashWindow => {
                t.smash_window = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::SpawnIframes => {
                t.spawn_iframes = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::SpotdodgeFrames => {
                t.spotdodge_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::TechIntang => {
                t.tech_intang = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::TechWindow => {
                t.tech_window = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::TechrollFrames => {
                t.techroll_frames = if let TuneValue::I64(x) = v { x } else { return }
            }
            TunePath::TechrollSpeed => {
                t.techroll_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::TumbleSpeed => {
                t.tumble_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::WalkSpeed => {
                t.walk_speed = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::WallBounce => {
                t.wall_bounce = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::WalljumpH => {
                t.walljump_h = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::WalljumpV => {
                t.walljump_v = if let TuneValue::F32(x) = v { x } else { return }
            }
            TunePath::Weight => t.weight = if let TuneValue::F32(x) = v { x } else { return },
            TunePath::ZoneExempt => {
                t.zone_exempt = if let TuneValue::Bool(x) = v {
                    x
                } else {
                    return;
                }
            } // sprefa:end
        }
    }
}

/// Per-field lens ZSTs (the typed half of principle 2): zero-size, `get`/`set`
/// on `Tune`, IDE-completable. One `struct` per leaf field.
pub mod lens {
    use crate::v1::Tune;

    /// Hand-written scaffolding: one call per leaf expands to a ZST lens. The
    /// generator emits the `tune_lens!(...)` calls between the markers below.
    macro_rules! tune_lens {
        ($name:ident, $field:ident, $ty:ty) => {
            #[derive(Copy, Clone)]
            pub struct $name;
            impl $name {
                pub fn get(self, t: &Tune) -> $ty {
                    t.$field
                }
                pub fn set(self, t: &mut Tune, v: $ty) {
                    t.$field = v;
                }
            }
        };
    }

    // sprefa:gen lenses
    tune_lens!(AcBoostAccel, ac_boost_accel, f32);
    tune_lens!(AcBoostMax, ac_boost_max, f32);
    tune_lens!(AcGravMult, ac_grav_mult, f32);
    tune_lens!(AcQbCd, ac_qb_cd, i64);
    tune_lens!(AcQbSpeed, ac_qb_speed, f32);
    tune_lens!(AcSpawnWeight, ac_spawn_weight, f32);
    tune_lens!(AirAccel, air_accel, f32);
    tune_lens!(AirFriction, air_friction, f32);
    tune_lens!(AirSpeed, air_speed, f32);
    tune_lens!(AirdodgeDrag, airdodge_drag, f32);
    tune_lens!(AirdodgeFrames, airdodge_frames, i64);
    tune_lens!(AirdodgeSpeed, airdodge_speed, f32);
    tune_lens!(AirjumpH, airjump_h, f32);
    tune_lens!(AirjumpV, airjump_v, f32);
    tune_lens!(AutohopDmg, autohop_dmg, f32);
    tune_lens!(BRevWindow, b_rev_window, i64);
    tune_lens!(BadgeSpawnWeight, badge_spawn_weight, f32);
    tune_lens!(BoosterCos, booster_cos, f32);
    tune_lens!(BoosterKb, booster_kb, f32);
    tune_lens!(BoosterLen, booster_len, f32);
    tune_lens!(BoosterStun, booster_stun, i64);
    tune_lens!(BufferFrames, buffer_frames, i64);
    tune_lens!(ChargeDmg, charge_dmg, f32);
    tune_lens!(ChargeMax, charge_max, i64);
    tune_lens!(ClankDiff, clank_diff, f32);
    tune_lens!(ClimbFrames, climb_frames, i64);
    tune_lens!(ClingFrames, cling_frames, i64);
    tune_lens!(CoyoteFrames, coyote_frames, i64);
    tune_lens!(CrawlSpeed, crawl_speed, f32);
    tune_lens!(DairThreshold, dair_threshold, f32);
    tune_lens!(DashInit, dash_init, f32);
    tune_lens!(DashTurnAccel, dash_turn_accel, f32);
    tune_lens!(DashWindow, dash_window, i64);
    tune_lens!(DashstopFriction, dashstop_friction, f32);
    tune_lens!(DiMaxAngle, di_max_angle, f32);
    tune_lens!(Fastfall, fastfall, f32);
    tune_lens!(FastfallThreshold, fastfall_threshold, f32);
    tune_lens!(FloorBounce, floor_bounce, f32);
    tune_lens!(FootstoolSpike, footstool_spike, f32);
    tune_lens!(FootstoolStun, footstool_stun, i64);
    tune_lens!(FootstoolV, footstool_v, f32);
    tune_lens!(FullhopV, fullhop_v, f32);
    tune_lens!(GetupFrames, getup_frames, i64);
    tune_lens!(GrabActive, grab_active, i64);
    tune_lens!(GrabHold, grab_hold, i64);
    tune_lens!(GrabMash, grab_mash, i64);
    tune_lens!(GrabRange, grab_range, f32);
    tune_lens!(GrabRecovery, grab_recovery, i64);
    tune_lens!(GrabStartup, grab_startup, i64);
    tune_lens!(Gravity, gravity, f32);
    tune_lens!(GroundAccel, ground_accel, f32);
    tune_lens!(GroundFriction, ground_friction, f32);
    tune_lens!(HandReachX, hand_reach_x, f32);
    tune_lens!(HandRise, hand_rise, f32);
    tune_lens!(InkBudget, ink_budget, f32);
    tune_lens!(InkCursorReach, ink_cursor_reach, f32);
    tune_lens!(InkLaunchSpeed, ink_launch_speed, f32);
    tune_lens!(InkSpawnWeight, ink_spawn_weight, f32);
    tune_lens!(ItemSpawnInterval, item_spawn_interval, i64);
    tune_lens!(ItemsOn, items_on, bool);
    tune_lens!(JumpHInit, jump_h_init, f32);
    tune_lens!(JumpHMax, jump_h_max, f32);
    tune_lens!(Jumpsquat, jumpsquat, i64);
    tune_lens!(KbHitstun, kb_hitstun, f32);
    tune_lens!(KbSpeed, kb_speed, f32);
    tune_lens!(KnockbackMult, knockback_mult, f32);
    tune_lens!(KnockdownFrames, knockdown_frames, i64);
    tune_lens!(LandingLag, landing_lag, i64);
    tune_lens!(LedgeCeil, ledge_ceil, f32);
    tune_lens!(LedgeFallEps, ledge_fall_eps, f32);
    tune_lens!(LedgeGrabR, ledge_grab_r, f32);
    tune_lens!(LedgeIntang, ledge_intang, i64);
    tune_lens!(LedgeLipBite, ledge_lip_bite, f32);
    tune_lens!(LedgeMinLen, ledge_min_len, f32);
    tune_lens!(LedgeReachDown, ledge_reach_down, f32);
    tune_lens!(LedgeReachX, ledge_reach_x, f32);
    tune_lens!(LedgejumpV, ledgejump_v, f32);
    tune_lens!(MaxAirDodges, max_air_dodges, i64);
    tune_lens!(MaxAirJumps, max_air_jumps, i64);
    tune_lens!(MaxFall, max_fall, f32);
    tune_lens!(MomentumCarry, momentum_carry, f32);
    tune_lens!(OneItemAtATime, one_item_at_a_time, bool);
    tune_lens!(PickupR, pickup_r, f32);
    tune_lens!(PickupReach, pickup_reach, f32);
    tune_lens!(PivotFrames, pivot_frames, i64);
    tune_lens!(PlatDropWindow, plat_drop_window, i64);
    tune_lens!(PummelBonus, pummel_bonus, i64);
    tune_lens!(PummelDamage, pummel_damage, f32);
    tune_lens!(ReboundFrames, rebound_frames, i64);
    tune_lens!(ReboundPush, rebound_push, f32);
    tune_lens!(RollFrames, roll_frames, i64);
    tune_lens!(RollSpeed, roll_speed, f32);
    tune_lens!(RunSpeed, run_speed, f32);
    tune_lens!(SaveScale, save_scale, f32);
    tune_lens!(SaveZeroPct, save_zero_pct, f32);
    tune_lens!(ShieldDecay, shield_decay, f32);
    tune_lens!(ShieldMax, shield_max, f32);
    tune_lens!(ShieldPush, shield_push, f32);
    tune_lens!(ShieldRegen, shield_regen, f32);
    tune_lens!(ShieldbreakFrames, shieldbreak_frames, i64);
    tune_lens!(ShieldstunPerDmg, shieldstun_per_dmg, f32);
    tune_lens!(ShipThrustAccel, ship_thrust_accel, f32);
    tune_lens!(ShorthopV, shorthop_v, f32);
    tune_lens!(SmashWindow, smash_window, i64);
    tune_lens!(SpawnIframes, spawn_iframes, i64);
    tune_lens!(SpotdodgeFrames, spotdodge_frames, i64);
    tune_lens!(TechIntang, tech_intang, i64);
    tune_lens!(TechWindow, tech_window, i64);
    tune_lens!(TechrollFrames, techroll_frames, i64);
    tune_lens!(TechrollSpeed, techroll_speed, f32);
    tune_lens!(TumbleSpeed, tumble_speed, f32);
    tune_lens!(WalkSpeed, walk_speed, f32);
    tune_lens!(WallBounce, wall_bounce, f32);
    tune_lens!(WalljumpH, walljump_h, f32);
    tune_lens!(WalljumpV, walljump_v, f32);
    tune_lens!(Weight, weight, f32);
    tune_lens!(ZoneExempt, zone_exempt, bool);
    // sprefa:end
}

// Leaf-set fingerprint (dl has no hash scalar -- this is the leaf COUNT, a cheap
// staleness hint the rail cross-checks).
// sprefa:gen hashline
// sprefa:hash leafcount=117
// sprefa:end

#[cfg(test)]
mod tests {
    use super::lens;
    use super::{TunePath, TuneValue};
    use crate::v1::Tune;

    // Hand-written scaffolding (outside the generated markers): proves the two
    // generated representations agree, and that a wrong-arm value is a no-op.
    #[test]
    fn lens_and_path_set_agree() {
        let mut a = Tune::default();
        let mut b = Tune::default();
        lens::Gravity.set(&mut a, 9.5);
        TunePath::Gravity.set(&mut b, TuneValue::F32(9.5));
        assert_eq!(lens::Gravity.get(&a), 9.5);
        assert_eq!(a.gravity, b.gravity);
    }

    #[test]
    fn wrong_value_arm_is_noop() {
        let mut t = Tune::default();
        let before = t.gravity;
        TunePath::Gravity.set(&mut t, TuneValue::I64(3)); // f32 field, i64 value
        assert_eq!(t.gravity, before);
    }

    #[test]
    fn i64_and_bool_leaves_route() {
        let mut t = Tune::default();
        TunePath::Jumpsquat.set(&mut t, TuneValue::I64(7));
        assert_eq!(t.jumpsquat, 7);
        TunePath::ItemsOn.set(&mut t, TuneValue::Bool(false));
        assert_eq!(t.items_on, false);
        lens::ClimbFrames.set(&mut t, 5);
        assert_eq!(lens::ClimbFrames.get(&t), 5);
    }
}
