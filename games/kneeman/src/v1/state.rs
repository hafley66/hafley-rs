//! Ground-truth enums for the per-fighter state machine: `CharState` (the ground/air/ledge
//! action states), the FSM's pure `Act`/`ThrowDir` output descriptors, and the input-buffer
//! trio `Action`/`Lane`/`Slot` (see `Fighter.buf` in `fighter.rs`). Split out of `lib.rs` (R5).

use crate::v1::Tune;
use crate::v1::Vector2;
use serde::{Deserialize, Serialize};

/// Ground/air/ledge action states. `frame` (in SimState) is the per-state timer that resets on
/// every transition, mirroring an animation frame — it gates the dash window, jumpsquat takeoff,
/// pivot, dodge length, landing lag, ledge intangibility, and getup.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CharState {
    Stand,       // idle, grounded
    Walk,        // tilt-speed ground move
    Dash,        // initial dash (frame-windowed burst)
    Run,         // full-speed run (dash accelerates into this)
    Turn,        // standing pivot
    Skid,        // run brake / slide to a stop
    Crouch,      // hold down, grounded
    JumpSquat,   // jump startup (universal 3f, shorthop decided here)
    Air,         // airborne: rising or falling
    Landing,     // touchdown lag
    Shield,      // guard
    SpotDodge,   // dodge in place (intangible)
    Roll,        // rolling dodge (intangible)
    AirDodge,    // directional air dodge (intangible; into ground = wavedash)
    LedgeHold,   // hanging on a ledge
    LedgeClimb,  // ledge getup
    Jab,         // grounded quick attack
    Nair,        // neutral aerial
    Fair,        // forward aerial (stick toward facing at the press)
    Bair,        // back aerial (stick behind facing; facing does NOT flip)
    Uair,        // up aerial: the juggle flip above the head
    Dair,        // down aerial: steep spike, drives the opponent down
    Dtilt,       // down-tilt (down + attack, grounded): low pothole poke that pops them up
    Ftilt,       // forward-tilt (held direction + attack): mid kick, faces the stick
    Utilt,       // up-tilt (held up + attack): anti-air swipe over the head
    Fsmash,      // forward smash (fresh flick + attack, or c-stick): plants and swings hard
    Usmash,      // up smash (up-flick + attack, c-stick up, or jump-cancel): vertical kill
    Dsmash,      // down smash (down-flick + attack, or c-stick down): low hit on both sides
    DashAttack,  // attack out of dash/run: lunges forward, keeps momentum
    Grab,        // grab attempt: short reach, heavy whiff recovery on miss
    GrabHold,    // holding a grabbed opponent (pummel / throw / they mash out)
    Grabbed,     // being held: frozen, mash inputs to break free
    Knockdown,   // floored after a hard launch (missed tech): lie, then get up
    Getup,       // rising from knockdown (intangible) -> Stand
    TechInPlace, // teched a landing in place (intangible recovery)
    TechRoll,    // teched/getup with a directional roll (intangible, moves)
    SpecialN,    // neutral-B
    SpecialS,    // side-B
    SpecialU,    // up-B (recovery; ends in Helpless if it finishes airborne)
    SpecialD,    // down-B
    Helpless,    // special-fall after an up-B: drift only, no actions until you land/ledge
    Launched,    // taking a hit: knockback slide + hitstun. Forced on connect so the interrupted
    // attacker's remaining hitbox windows never fire (attack_for(Launched) == None).
    Crawl,       // crouched creep (hold down + a direction); attacks from here read as dtilt
    LedgeAttack, // ledge getup attack: planted on the lip swinging inward (intangible startup)
    LedgeRoll,   // ledge getup roll: onto the stage at tech-roll speed, intangible throughout
    GetupAttack, // rising swing off the floor: front then back hit (intangible startup)
    Rebound,     // clank recoil: the cancelled move staggers back, unactionable briefly
    ShieldBreak, // shield shattered: long dizzy, fully punishable; hp restores when it ends
    // APPENDED (bincode positional -- keep last): wall tech. A tumbling body that techs a WALL is
    // NOT on a floor, so it gets its own AIRBORNE intangible state instead of borrowing the grounded
    // `TechInPlace` (which snapped the fighter to the floor -- the wall-tech teleport, debt #4).
    TechWall,
    // Appended: landing phases retain their originating loadout slot without a Fighter field.
    SpecialLandN,
    SpecialLandS,
    SpecialLandU,
    SpecialLandD,
}

/// The "next action" a fighter's state machine emits each frame: a pure descriptor of an effect it
/// can't apply alone because it needs the whole `SimState` (the item array). `reduce_next_state` returns one;
/// `step` actuates it via `apply_act`. The input stream is never mutated — the FSM just describes.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Act {
    None,
    /// held gun + attack: spawn its projectile. `auto` = held, not a fresh tap (weaker).
    /// `aim` = unit c-stick direction when the gun aims (ItemLogic.aims) and the stick is
    /// deflected; ZERO = fire along facing. `aim_y` is the raw MAIN-stick y at the press
    /// (`InputFrame.aim_y`, -1 up..+1 down) -- unused by most guns; TetrisDropper quantizes
    /// it into a tetromino pick (items/tetris_drop.rs).
    Fire {
        auto: bool,
        aim: Vector2,
        aim_y: f32,
    },
    Drop,   // held item + neutral grab: detach to the ground (the gentle toss)
    Pickup, // empty hands + attack over an item: claim it (else attack jabs)
    Draw,   // held pen + attack: lay a node on the owner's ink path this frame
    Throw {
        dir: ThrowDir,
    }, // held item + directional grab: launch it as a live projectile
    /// airborne jump press directly above another body: hop off their head. Cross-fighter,
    /// so the FSM only describes it; `apply_act` pops the jumper and staggers the victim.
    Footstool {
        victim: i8,
    },
    /// AC'd + c-stick deflected + arm off cooldown: fire the rolled arm weapon along `aim`.
    /// Spawns into the shared item array, so the FSM only describes it (like Fire).
    ArmFire {
        aim: Vector2,
    },
    /// Empty-handed interact (attack/grab) over a mounted station in reach: OCCUPY it — lock this
    /// fighter to `SHIP_STATIONS[station]`. Cross-fighter ("already taken?"), so the FSM only
    /// describes it; `apply_act` sets `Fighter.station` iff no other rider holds that anchor.
    Occupy {
        station: i8,
    },
}

/// Directional item throw (Smash-style). Neutral grab is the soft toss (`Act::Drop`); a real stick
/// direction routes here so the item flies as an armed projectile keyed to this direction.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ThrowDir {
    Up,
    Down,
    Forward,
    Back,
}

pub(crate) fn airborne(st: CharState) -> bool {
    matches!(
        st,
        CharState::Air
            | CharState::AirDodge
            | CharState::Nair
            | CharState::Fair
            | CharState::Bair
            | CharState::Uair
            | CharState::Dair
            | CharState::Helpless
            | CharState::TechWall // wall tech: stuck to the wall in the air, not on a floor
    )
}

/// The five aerial-attack states (drift + gravity while swinging, land-cancelable).
pub(crate) fn is_aerial_attack(st: CharState) -> bool {
    matches!(
        st,
        CharState::Nair | CharState::Fair | CharState::Bair | CharState::Uair | CharState::Dair
    )
}

/// Every input edge that can be buffered, as one type. Recorded on the button edge with the aim at
/// that moment, consumed when the state machine reaches a point where it can act — this is what
/// makes wavedash / jump-out-of-lag / the down-diagonal feel reliable instead of frame-perfect.
/// `window` is the only place a buffer length is decided, dispatched by a match (the enum's job —
/// no bit tricks): the lookahead edges share the live Tune window; `Grab` is 0 (press-frame only,
/// as today) but expressible, the seam to give it a real buffer later.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Action {
    #[default]
    None,
    Jump,
    ShortHop,
    AirDodge,
    Aerial,
    Attack,
    Strong, // c-stick flick on the ground: a smash in the flicked direction
    Grab,
    Special,
}

impl Action {
    /// Display label for the debug panel.
    pub fn name(self) -> &'static str {
        match self {
            Action::None => "—",
            Action::Jump => "JUMP",
            Action::ShortHop => "SHORTHOP",
            Action::AirDodge => "AIRDODGE",
            Action::Aerial => "AERIAL",
            Action::Attack => "ATTACK",
            Action::Strong => "STRONG",
            Action::Grab => "GRAB",
            Action::Special => "SPECIAL",
        }
    }

    pub(crate) fn window(self, t: &Tune) -> i64 {
        match self {
            Action::None | Action::Grab => 0,
            Action::Jump
            | Action::ShortHop
            | Action::AirDodge
            | Action::Aerial
            | Action::Attack
            | Action::Strong
            | Action::Special => t.buffer_frames,
        }
    }
}

/// Buffer lanes. Each lane holds at most one pending action and lanes coexist, so a jump and an
/// aerial pressed together for the auto-short-hop both survive. The `Movement` lane carries
/// whichever of jump / short-hop / air-dodge was pressed most recently (they're mutually-exclusive
/// intents — newest wins); the rest are single-action.
#[repr(usize)]
#[derive(Copy, Clone)]
pub(crate) enum Lane {
    Movement,
    Aerial,
    Attack,
    Strong, // grounded c-stick: pending smash, aim = the c-stick at the flick
    Grab,
    Special,
}
pub(crate) const N_LANE: usize = 6;

/// One lane's pending action. `timer == 0` (or `action == None`) means empty; while live, `aim` is
/// the stick captured at the press and, on the movement lane, refreshed within the window — the
/// diagonal that lets a buffered air-dodge keep its latest direction.
#[derive(Copy, Clone, PartialEq, Default, Debug, Serialize, Deserialize)]
pub struct Slot {
    pub action: Action,
    pub timer: i64,
    pub aim: Vector2,
}
