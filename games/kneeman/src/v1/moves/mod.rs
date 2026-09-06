//! Moves, grouped by kind. This module holds the shared normal-attack data (frame data + hitboxes +
//! knockback records) that grounded attacks, aerials, and tilts all draw from; `special` and `throw`
//! are submodules for those kinds. Moves are DATA indexed by `CharState` (see `attack_for`), so the
//! sim stays `Copy` + deterministic. Re-exported at the crate root.
//!
//! Hitbox model (Brawl-shaped): a move is N hitboxes, each owning its OWN frame window
//! (`start`/`len` relative to state start) on the shared `f.frame` clock. That is what makes
//! multi-hit + sequenced-timing moves (the 3-punch jab, the Knee Man stomp) authorable without a
//! per-move FSM state. Lowest live `id` wins per victim (sweetspot beats sourspot). Knockback is the
//! community / Project-M formula (see `knockback_units`), NOT the Melee decomp. See
//! `plans/hitbox-modeling.md`.

pub mod special;
pub mod throw;
pub use special::*;
pub use throw::*;

use self::special::special_slot;
use crate::v1::{CharState, DUMMY_R, ECB_HALF_H, Fighter, Tune, Vector2};
use serde::{Deserialize, Serialize};

/// Hitboxes per move (Brawl-ish cap). Fixed so `AttackData` stays `Copy` + snapshot-cheap.
pub const MAX_HB: usize = 4;

/// One hitbox: a circle offset from the fighter (x flipped by facing), live only on its own frame
/// window `[start, start + len)`. A move is up to `MAX_HB` of these on the shared state clock, so
/// sweetspot/sourspot, sex-kicks, and rapid multi-hit are all "more boxes", never new states.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Hitbox {
    pub id: u8,       // priority; lowest LIVE overlapping id wins per victim (sweetspot first)
    pub start: i64,   // first active frame, relative to state start
    pub len: i64,     // active duration; live for [start, start + len). 0 = inert box
    pub off: Vector2, // center offset from fighter pos (x is forward, flipped by facing)
    pub r: f32,       // radius
    pub damage: f32,  // % added on connect
    pub angle: f32,   // launch angle° (0 fwd, 90 up, negative = spike)
    pub bkb: f32,     // base knockback (community/PM BKB), in KB units
    pub kbg: f32,     // knockback growth (community/PM KBG; % scaling of the ramp)
    pub set_kb: f32,  // weight-independent fixed knockback (KB units); 0 = use the growth formula
    pub transcendent: bool, // skips the clank check (projectiles; aerials are de-facto transcendent)
    pub refresh: i64,       // frames a victim is immune to THIS box after a connect (multi-hit gap)
}

impl Hitbox {
    pub const NONE: Self = Self {
        id: 0,
        start: 0,
        len: 0,
        off: Vector2::ZERO,
        r: 0.0,
        damage: 0.0,
        angle: 0.0,
        bkb: 0.0,
        kbg: 0.0,
        set_kb: 0.0,
        transcendent: false,
        refresh: 0,
    };

    /// Is this box live at `frame` (relative to state start)? Inert boxes (`len == 0`) never are.
    #[inline]
    pub fn live_at(&self, frame: i64) -> bool {
        self.len > 0 && frame >= self.start && frame < self.start + self.len
    }
}

/// Landing-interrupt knob (queue-2026-07-03 item 3): what an in-progress attack does when its
/// airborne state crosses onto ground before the move's own timeline ends. `Continue` keeps the
/// act running in place (Falcon Punch style -- a special launched in the air keeps swinging once
/// planted, per the existing grounded/aerial split in `run_special`/`integrate_collide`);
/// `ResetToLanding` aborts it into the normal `Landing` state (Lucas side-B style). Every
/// existing move defaults to `ResetToLanding`: today, EVERY air->ground touch during an attack
/// forces `Landing` unconditionally (za_warudo's `integrate_collide`), so that is the
/// behavior-preserving default -- `Continue` is new, opt-in per move via `land_transition`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LandCancel {
    Continue,
    ResetToLanding,
}

/// One attack's frame data: a lead-in, up to `MAX_HB` windowed hitboxes (id-ordered), and endlag.
/// The boxes drive both the hits and (via `f.frame`) the animation; `total()` sets the FSM state
/// length so the timer and the hitboxes stay in lockstep.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttackData {
    pub startup: i64, // animation lead-in (no box before this); boxes key off the same f.frame
    pub recovery: i64, // endlag after the last box closes
    pub boxes: [Hitbox; MAX_HB], // id-ordered, fixed cap
    pub nbox: u8,     // how many of `boxes` are real (the rest are Hitbox::NONE)
    /// Air->ground landing-interrupt knob; see `LandCancel`. Appended at the struct's end.
    pub land_cancel: LandCancel,
}

impl AttackData {
    /// Helper: build from a slice of boxes, padding to `MAX_HB` with inert boxes.
    pub const fn new(startup: i64, recovery: i64, boxes: [Hitbox; MAX_HB], nbox: u8) -> Self {
        Self {
            startup,
            recovery,
            boxes,
            nbox,
            land_cancel: LandCancel::ResetToLanding,
        }
    }

    /// Single-window attack: one box opening at `startup`. The common shape (jab/dair/dash/specials).
    pub const fn one(startup: i64, active: i64, recovery: i64, hb: Hitbox) -> Self {
        let mut b = hb;
        b.id = 0;
        b.start = startup;
        b.len = active;
        Self {
            startup,
            recovery,
            boxes: [b, Hitbox::NONE, Hitbox::NONE, Hitbox::NONE],
            nbox: 1,
            land_cancel: LandCancel::ResetToLanding,
        }
    }

    /// The real boxes (drops the inert padding).
    pub fn live_boxes(&self) -> &[Hitbox] {
        &self.boxes[..self.nbox as usize]
    }

    /// Last frame any box is still live (= max start+len). Used both by `total()` and by moves that
    /// gate motion on "still swinging" (the dash-attack slide).
    pub fn active_end(&self) -> i64 {
        self.live_boxes()
            .iter()
            .map(|b| b.start + b.len)
            .max()
            .unwrap_or(self.startup)
    }

    /// FSM state length: last box close, then recovery.
    pub fn total(&self) -> i64 {
        self.active_end() + self.recovery
    }

    /// The lowest-id box live at `frame` whose window contains it. `None` outside every window.
    /// This is the per-victim "which window pays out" pick (id priority).
    pub fn box_at(&self, frame: i64) -> Option<&Hitbox> {
        self.live_boxes()
            .iter()
            .filter(|b| b.live_at(frame))
            .min_by_key(|b| b.id)
    }

    // baseline definitions; live copies live in Tune so the panel can edit them.
    // PM/community-flavored bkb/kbg/angle (NOT the Melee decomp). Knockback runs through
    // `knockback_units`; aerials author `transcendent: true` (aerials don't clank).
    // 3-punch jab autocombo: one press throws three sequenced hits on the shared frame clock. The
    // first two are weak SET-knockback links (a jab-lock pop that holds the victim regardless of %),
    // the third is the launcher with real growth. Three boxes, three windows, no extra FSM state.
    pub(crate) const JAB: Self = Self {
        startup: 3,
        recovery: 12,
        boxes: [
            Hitbox {
                id: 0,
                start: 3,
                len: 2,
                off: Vector2::new(44.0, -64.0),
                r: 32.0,
                damage: 3.0,
                angle: 80.0,
                bkb: 8.0,
                kbg: 0.0,
                set_kb: 6.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 9,
                len: 2,
                off: Vector2::new(48.0, -64.0),
                r: 32.0,
                damage: 3.0,
                angle: 80.0,
                bkb: 8.0,
                kbg: 0.0,
                set_kb: 7.0,
                transcendent: false,
                refresh: 0,
            },
            // launcher: pops near-vertical so a 0% triple-jab leaves the victim above you, in range
            // to follow up with a rising aerial (the sex-kick juggle). Low growth keeps it a true
            // combo at low %, not a blow-away.
            Hitbox {
                id: 2,
                start: 15,
                len: 3,
                off: Vector2::new(54.0, -60.0),
                r: 36.0,
                damage: 6.0,
                angle: 78.0,
                bkb: 30.0,
                kbg: 40.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox::NONE,
        ],
        nbox: 3,
        land_cancel: LandCancel::ResetToLanding,
    };
    // neutral aerial — a sex-kick: a strong early pop, then a weak lingering tail at the same limb.
    // Two boxes, same off/r, different windows + payoff. Aerial => transcendent.
    pub(crate) const NAIR: Self = Self {
        startup: 5,
        recovery: 14,
        boxes: [
            Hitbox {
                id: 0,
                start: 5,
                len: 5,
                off: Vector2::new(26.0, -60.0),
                r: 52.0,
                damage: 8.0,
                angle: 45.0,
                bkb: 22.0,
                kbg: 42.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 10,
                len: 7,
                off: Vector2::new(26.0, -60.0),
                r: 52.0,
                damage: 5.0,
                angle: 30.0,
                bkb: 14.0,
                kbg: 26.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
    // Knee Man's aerial stomp (Captain-Falcon-flavored): contraction startup, then the foot sweeps
    // DOWN through space. Three sequential id:0 spike boxes track the foot from tucked (small +y,
    // near waist) to fully extended (large +y, well below body), each shifted further down-and-out
    // than the last. The victim connects with ONE stomp — hitstun carries them clear before the next
    // window opens. Miss the spike timing entirely and the late id:1 sourspot pops them up-and-away.
    // Aerial ⇒ transcendent.
    pub(crate) const DAIR: Self = Self {
        startup: 6,
        recovery: 8,
        boxes: [
            // foot at tuck-exit: spike opens, near waist height. (radii scaled 1.5x for reach)
            Hitbox {
                id: 0,
                start: 6,
                len: 2,
                off: Vector2::new(10.0, 5.0),
                r: 54.0,
                damage: 12.0,
                angle: -80.0,
                bkb: 18.0,
                kbg: 36.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            // foot mid-extension: same id, next window, driven further down-and-out.
            Hitbox {
                id: 0,
                start: 8,
                len: 2,
                off: Vector2::new(14.0, 38.0),
                r: 57.0,
                damage: 12.0,
                angle: -80.0,
                bkb: 18.0,
                kbg: 36.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            // foot near full extension: spike closes out.
            Hitbox {
                id: 0,
                start: 10,
                len: 2,
                off: Vector2::new(18.0, 66.0),
                r: 60.0,
                damage: 12.0,
                angle: -80.0,
                bkb: 18.0,
                kbg: 36.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            // sourspot tail: foot fully extended, sends up-and-away on whiffed spike timing.
            Hitbox {
                id: 1,
                start: 12,
                len: 5,
                off: Vector2::new(20.0, 74.0),
                r: 54.0,
                damage: 6.0,
                angle: 60.0,
                bkb: 10.0,
                kbg: 22.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
        ],
        nbox: 4,
        land_cancel: LandCancel::ResetToLanding,
    };
    pub(crate) const DASH_ATTACK: Self = Self::one(
        8,
        38,
        6,
        Hitbox {
            damage: 11.0,
            off: Vector2::new(100.0, -58.0),
            r: 54.0,
            angle: 30.0,
            bkb: 24.0,
            kbg: 39.0,
            ..Hitbox::NONE
        },
    );
    // Game & Watch "Manhole" down-tilt: a fast low pop with a lingering second window (juggle setup).
    pub(crate) const DTILT: Self = Self {
        startup: 5,
        recovery: 10,
        boxes: [
            Hitbox {
                id: 0,
                start: 5,
                len: 3,
                off: Vector2::new(40.0, 2.0),
                r: 30.0,
                damage: 6.0,
                angle: 80.0,
                bkb: 14.0,
                kbg: 32.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 8,
                len: 6,
                off: Vector2::new(40.0, 2.0),
                r: 30.0,
                damage: 4.0,
                angle: 88.0,
                bkb: 10.0,
                kbg: 22.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
    // forward aerial — the Knee: a short early sweetspot that sends hard and low, then a long
    // sour tail that just taps. Two boxes at the extended leg, id 0 wins while both are live.
    pub(crate) const FAIR: Self = Self {
        startup: 7,
        recovery: 16,
        boxes: [
            Hitbox {
                id: 0,
                start: 7,
                len: 3,
                off: Vector2::new(58.0, -56.0),
                r: 44.0,
                damage: 16.0,
                angle: 32.0,
                bkb: 28.0,
                kbg: 78.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 10,
                len: 10,
                off: Vector2::new(56.0, -54.0),
                r: 38.0,
                damage: 6.0,
                angle: 45.0,
                bkb: 12.0,
                kbg: 20.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
    // back aerial — quick, strong, behind you (negative off.x: hitbox_center flips by facing).
    // Facing does NOT change; this is the reward for attacking with your back to them.
    pub(crate) const BAIR: Self = Self::one(
        5,
        4,
        12,
        Hitbox {
            damage: 12.0,
            off: Vector2::new(-54.0, -60.0),
            r: 44.0,
            angle: 35.0,
            bkb: 24.0,
            kbg: 62.0,
            transcendent: true,
            ..Hitbox::NONE
        },
    );
    // up aerial — the juggle flip: a front-up window sweeping to a behind-up window overhead.
    pub(crate) const UAIR: Self = Self {
        startup: 6,
        recovery: 12,
        boxes: [
            Hitbox {
                id: 0,
                start: 6,
                len: 4,
                off: Vector2::new(20.0, -110.0),
                r: 46.0,
                damage: 10.0,
                angle: 75.0,
                bkb: 20.0,
                kbg: 48.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 10,
                len: 4,
                off: Vector2::new(-16.0, -114.0),
                r: 42.0,
                damage: 8.0,
                angle: 88.0,
                bkb: 16.0,
                kbg: 40.0,
                set_kb: 0.0,
                transcendent: true,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
    // forward tilt — a mid-range planted kick; quick, safe-ish, sends at a combo-breaking 35°.
    pub(crate) const FTILT: Self = Self::one(
        6,
        4,
        14,
        Hitbox {
            damage: 9.0,
            off: Vector2::new(72.0, -48.0),
            r: 40.0,
            angle: 35.0,
            bkb: 20.0,
            kbg: 40.0,
            ..Hitbox::NONE
        },
    );
    // up tilt — anti-air swipe over the head: pops near-vertical, the juggle starter.
    pub(crate) const UTILT: Self = Self::one(
        7,
        6,
        16,
        Hitbox {
            damage: 9.0,
            off: Vector2::new(6.0, -118.0),
            r: 46.0,
            angle: 84.0,
            bkb: 22.0,
            kbg: 46.0,
            ..Hitbox::NONE
        },
    );
    // forward smash — the planted haymaker: slow, huge payoff, the grounded kill move.
    pub(crate) const FSMASH: Self = Self::one(
        13,
        4,
        24,
        Hitbox {
            damage: 17.0,
            off: Vector2::new(80.0, -56.0),
            r: 50.0,
            angle: 40.0,
            bkb: 64.0,
            kbg: 176.0,
            ..Hitbox::NONE
        },
    );
    // up smash — vertical killer above the head; slides with jump-cancel momentum (PM).
    pub(crate) const USMASH: Self = Self::one(
        9,
        6,
        26,
        Hitbox {
            damage: 15.0,
            off: Vector2::new(0.0, -122.0),
            r: 54.0,
            angle: 86.0,
            bkb: 56.0,
            kbg: 168.0,
            ..Hitbox::NONE
        },
    );
    // ledge getup attack — climbs the lip and sweeps inward: one solid low box in front. The
    // startup is intangible (za_warudo gates it), the payoff clears a ledge-trapper.
    pub(crate) const LEDGE_ATTACK: Self = Self::one(
        14,
        4,
        18,
        Hitbox {
            damage: 9.0,
            off: Vector2::new(64.0, -40.0),
            r: 48.0,
            angle: 40.0,
            bkb: 22.0,
            kbg: 40.0,
            ..Hitbox::NONE
        },
    );
    // knockdown getup attack — the rising sweep: front window then back window, so it covers
    // both sides of the floored body. Intangible through startup (za_warudo gates it).
    pub(crate) const GETUP_ATTACK: Self = Self {
        startup: 12,
        recovery: 16,
        boxes: [
            Hitbox {
                id: 0,
                start: 12,
                len: 3,
                off: Vector2::new(52.0, -44.0),
                r: 44.0,
                damage: 7.0,
                angle: 45.0,
                bkb: 20.0,
                kbg: 32.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 16,
                len: 3,
                off: Vector2::new(-52.0, -44.0),
                r: 44.0,
                damage: 7.0,
                angle: 135.0, // past 90°: sends backward relative to facing
                bkb: 20.0,
                kbg: 32.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
    // down smash — front hit then back hit, low and semi-spiky send: the ledge-clearer.
    pub(crate) const DSMASH: Self = Self {
        startup: 8,
        recovery: 22,
        boxes: [
            Hitbox {
                id: 0,
                start: 8,
                len: 3,
                off: Vector2::new(58.0, -10.0),
                r: 42.0,
                damage: 13.0,
                angle: 24.0,
                bkb: 52.0,
                kbg: 140.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox {
                id: 1,
                start: 13,
                len: 3,
                off: Vector2::new(-58.0, -10.0),
                r: 42.0,
                damage: 13.0,
                angle: 156.0, // past 90°: sends backward-and-low relative to facing
                bkb: 52.0,
                kbg: 140.0,
                set_kb: 0.0,
                transcendent: false,
                refresh: 0,
            },
            Hitbox::NONE,
            Hitbox::NONE,
        ],
        nbox: 2,
        land_cancel: LandCancel::ResetToLanding,
    };
}

/// Community / Project-M knockback in KB units (NOT the Melee decomp). `p` = victim % AFTER the hit's
/// damage is added, `d` = hit damage, `w` = victim weight. `bkb`/`kbg`/`set_kb` come off the box.
/// The caller turns units into px/s (`* Tune.kb_speed`) and hitstun (`floor(units * kb_hitstun)`),
/// so "hitstun = floor(0.4 * KB)" stays literal.
pub fn knockback_units(p: f32, d: f32, w: f32, hb: &Hitbox) -> f32 {
    if hb.set_kb > 0.0 {
        // weight-independent fixed knockback: jab-lock / multi-hit links stay reliable.
        return hb.set_kb;
    }
    (((p / 10.0 + p * d / 20.0) * (200.0 / (w + 100.0)) * 1.4 + 18.0) * (hb.kbg / 100.0)) + hb.bkb
}

pub fn attack_for(t: &Tune, st: CharState) -> Option<AttackData> {
    match st {
        CharState::Jab => Some(t.jab),
        CharState::Nair => Some(t.nair),
        CharState::Fair => Some(t.fair),
        CharState::Bair => Some(t.bair),
        CharState::Uair => Some(t.uair),
        CharState::Dair => Some(t.dair),
        CharState::Dtilt => Some(t.dtilt),
        CharState::Ftilt => Some(t.ftilt),
        CharState::Utilt => Some(t.utilt),
        CharState::Fsmash => Some(t.fsmash),
        CharState::Usmash => Some(t.usmash),
        CharState::Dsmash => Some(t.dsmash),
        CharState::DashAttack => Some(t.dash_attack),
        CharState::LedgeAttack => Some(t.ledge_attack),
        CharState::GetupAttack => Some(t.getup_attack),
        _ => special_slot(st).map(|s| t.specials[s].hit),
    }
}

/// What `st` becomes the instant it touches ground mid-attack (queue-2026-07-03 item 3): the
/// one FSM consult site for `LandCancel`. A state with no `AttackData` (Air/AirDodge/Helpless --
/// the non-attack airborne states) has no move to interrupt, so it always resets to `Landing`,
/// matching today's unconditional landing transition. An attack state defers to its own
/// `AttackData.land_cancel`.
pub fn land_transition(t: &Tune, st: CharState) -> CharState {
    match attack_for(t, st) {
        Some(atk) if atk.land_cancel == LandCancel::Continue => st,
        _ => CharState::Landing,
    }
}

/// Smash-charge payoff: 1.0 uncharged, `t.charge_dmg` at a full bank (linear between).
/// Knockback scales through the formula since damage feeds it. Non-smash states are 1.0
/// — a stale `charge` from an interrupted smash can't leak into other moves.
pub fn charge_mult(f: &Fighter, t: &Tune) -> f32 {
    match f.state {
        CharState::Fsmash | CharState::Usmash | CharState::Dsmash => {
            1.0 + (f.charge as f32 / t.charge_max.max(1) as f32) * (t.charge_dmg - 1.0)
        }
        _ => 1.0,
    }
}

/// Pick which aerial comes out from the aim captured at the attack press, relative to `facing`.
/// Cardinal gate: a vertical pick needs the stick steeper than horizontal, so holding forward for
/// drift with a touch of down still gives the horizontal aerial. Toward facing = fair, away = bair
/// (facing does NOT flip — bair is the back hit, not a turnaround). Near-neutral = nair.
pub fn aerial_for(aim: Vector2, facing: f32, t: &Tune) -> CharState {
    let steep = aim.y.abs() > aim.x.abs();
    if aim.y >= t.dair_threshold && steep {
        CharState::Dair
    } else if -aim.y >= t.dair_threshold && steep {
        CharState::Uair
    } else if aim.x * facing >= 0.3 {
        CharState::Fair
    } else if aim.x * facing <= -0.3 {
        CharState::Bair
    } else {
        CharState::Nair
    }
}

/// World-space center + radius of a hitbox for the attacker's current facing.
#[inline]
pub fn hitbox_center(f: &Fighter, hb: &Hitbox) -> (Vector2, f32) {
    (f.pos + Vector2::new(hb.off.x * f.facing, hb.off.y), hb.r)
}

/// Every hitbox live THIS frame for an attacking state, in world space (id-ordered). Slots past
/// `nbox`/inactive windows are `None`. The shell/debug draw iterates this; `resolve_combat` uses
/// `box_at` for the id-priority pick.
pub fn live_hitboxes(f: &Fighter, t: &Tune) -> [Option<(Vector2, f32)>; MAX_HB] {
    let mut out = [None; MAX_HB];
    if let Some(atk) = attack_for(t, f.state) {
        for (i, b) in atk.live_boxes().iter().enumerate() {
            if b.live_at(f.frame) {
                out[i] = Some(hitbox_center(f, b));
            }
        }
    }
    out
}

/// The lowest-id hitbox live this frame, in world space (None if the move has no live box now).
/// Kept for the single-shape debug draw (shell + web); combat uses `box_at` directly.
pub fn active_hitbox(f: &Fighter, t: &Tune) -> Option<(Vector2, f32)> {
    let atk = attack_for(t, f.state)?;
    atk.box_at(f.frame).map(|b| hitbox_center(f, b))
}

/// A fighter's hurtbox: a circle whose center height + radius shift with the current state, so the
/// silhouette an attack lands on matches the pose. Base is mid-body (one ECB half-height above the
/// feet) at the full body radius; crouch/knockdown duck low and shrink (you can whiff a jab over a
/// crouch), aerials tuck the body a touch higher. Single circle keeps the overlap test cheap and the
/// debug draw one shape.
pub fn hurtbox(f: &Fighter) -> (Vector2, f32) {
    let base = -ECB_HALF_H;
    let (dy, r) = match f.state {
        // ducking: center drops toward the feet, body pulls in — the classic crouch-under.
        CharState::Crouch | CharState::Crawl | CharState::Dtilt => (base * 0.55, DUMMY_R * 0.82),
        // floored: lying low and compact until getup.
        CharState::Knockdown => (base * 0.40, DUMMY_R * 0.88),
        // airborne poses tuck the legs up: center rides a little higher than standing.
        CharState::Nair
        | CharState::Fair
        | CharState::Bair
        | CharState::Uair
        | CharState::Dair
        | CharState::Air
        | CharState::Helpless => (base - 6.0, DUMMY_R),
        _ => (base, DUMMY_R),
    };
    (f.pos + Vector2::new(0.0, dy), r)
}
