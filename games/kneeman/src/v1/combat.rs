//! One hit, one ritual (plans/body-bus.md §1). `strike` computes what a connect does
//! (`Launch`) from the shared knockback formula; the receiver decides what that means
//! (`PunchableFace::absorb`). Folds the launch math previously copy-pasted across
//! `resolve_combat` / `ink_hits_fighters` / item throw / bolt / `explode`.
//!
//! Trait rule: every impl is a ZST view over a struct already in SimState; every method
//! body is a field read or a table read. Monomorphized, no dyn, rollback untouched.

use crate::v1::geo::{self, Iso, Shape};
use crate::v1::moves::{Hitbox, attack_for, charge_mult, hitbox_center, hurtbox, knockback_units};
use crate::v1::physics::{HITLAG_PER_DMG, apply_di, sign};
use crate::v1::state::is_aerial_attack;
use crate::v1::{CharState, Fighter, SimState, Tune, Vector2, stage};

/// Why a hit didn't land — or that it landed on a raised shield. `Invuln`/`Intangible`
/// whiff entirely; `Shield` routes the connect down the block path in `strike` (shield
/// damage + guard stun + pushback instead of a launch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guard {
    Open,
    Invuln,               // spawn i-frames: the hit whiffs entirely
    Intangible,           // dodge: passes through
    Shield { left: f32 }, // raised guard: shield health remaining (Tune.shield_max scale)
}

/// What one connect does to one receiver, fully computed before anything mutates.
/// The ritual's output: Copy, throwaway, lives for one `strike` call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Launch {
    pub dmg: f32,
    /// The formula's scalar launch speed (`|vel|` before rounding): receivers with their
    /// own launch thresholds (ink's unlock-vs-shake) compare against this exactly.
    pub speed: f32,
    pub vel: Vector2,
    pub hitstun: i64,
    pub tumble: bool,
    pub hitlag: i64,
    /// Force `Launched` + `frame = 0` (the move-cancel interrupt). Melee hits and the
    /// ink truck do this; item hits (throw/bolt/blast) historically don't cancel the
    /// victim's mid-swing move. TODO(body-bus): unify once that's a deliberate call.
    pub interrupt: bool,
    /// The connect landed on a raised shield: the receiver's `absorb` runs the block
    /// path (shield damage + guard stun + pushback), and the attacker-side caller skips
    /// victim DI. Attacker hitlag still applies — a blocked hit still pops.
    pub blocked: bool,
}

/// Where the launch vector comes from: the only geometry that varied across the rituals.
/// `resolve` returns a unit direction; `strike` scales it by the formula's speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// Launch along the box's angle, mirrored by attacker facing (melee, throws, bolts).
    Angle { deg: f32, facing: f32 },
    /// Away from a blast center with an upward pop bias (explosions).
    Radial { center: Vector2, up: f32 },
    /// Along the hitter's own travel, up-biased (a flying ink body's impact velocity).
    Carry { vel: Vector2, up: f32 },
}

impl Aim {
    /// Unit launch direction toward/away from `target` (the receiver's hurt center;
    /// only `Radial` reads it).
    pub fn resolve(&self, target: Vector2) -> Vector2 {
        match *self {
            Aim::Angle { deg, facing } => {
                let ang = deg.to_radians();
                Vector2::new(ang.cos() * facing, -ang.sin())
            }
            Aim::Radial { center, up } => {
                let d = target - center;
                let radial = if d.length() > 1.0 {
                    d / d.length()
                } else {
                    Vector2::new(0.0, -1.0)
                };
                (radial + Vector2::new(0.0, -up)).normalize_or_zero()
            }
            Aim::Carry { vel, up } => {
                (vel.normalize_or_zero() + Vector2::new(0.0, -up)).normalize_or_zero()
            }
        }
    }
}

/// The per-source knobs that varied across the rituals beyond the aim. Damage arrives
/// pre-computed because every source has its own policy (autohop scale, auto-fire
/// weakness, blast falloff, impact speed); `kb_scale` additionally scales the formula
/// OUTPUT (explosions fall off knockback by distance, not just damage).
#[derive(Clone, Copy, Debug)]
pub struct Swing<'h> {
    pub hb: &'h Hitbox,
    pub dmg: f32,
    pub kb_scale: f32,
    pub hitlag_bonus: i64, // +4 melee-grade impact freeze, +2 projectile-grade
    pub interrupt: bool,
    pub aim: Aim,
}

/// Anything that takes a hit. Melee precedent: Sandbag (hurt shapes + damage% + weight
/// + KB response, no FSM, never promoted to fighter). Impls: Fighter today; InkPath and
/// Item as body-bus steps land.
pub trait PunchableFace {
    /// Broadphase circle. Cull with this, never resolve with it.
    fn bound(&self) -> (Vector2, f32);
    /// Narrowphase hurt volumes, world space. Fighter emits one ball today, shaped
    /// hurtboxes later; ink emits its polyline as capsules; items a point ball.
    fn hurt_shapes(&self, out: &mut impl FnMut(Iso, Shape));
    /// Accumulated damage % (the KB formula's `p`). Invincible-punchables return 0 forever.
    fn percent(&self) -> f32;
    /// The KB formula's weight term `w`.
    fn heft(&self, t: &Tune) -> f32;
    fn guard(&self) -> Guard;
    /// Apply the launch. The impl's policy lives here: fighter takes state + stun; ink
    /// decides unlock-vs-shake; a bomb may detonate. `contact` is the world point the
    /// hit landed at (torque for rigid bodies; fighters ignore it).
    fn absorb(&mut self, l: Launch, contact: Vector2, t: &Tune);
}

/// Blast falloff: full strength at the center, half at the rim.
#[inline]
pub fn blast_falloff(dist: f32, radius: f32) -> f32 {
    1.0 - 0.5 * (dist / radius)
}

/// The one launch ritual. Whiffs (returns `None`) on i-frames; on `Open` computes
/// damage -> knockback units -> launch vector -> hitstun/tumble/hitlag and hands the
/// receiver its `Launch`; on `Shield` the same `Launch` goes out tagged `blocked` and
/// the receiver's `absorb` runs the block path instead. Victim-side DI is NOT applied
/// here: aim input is fighter-only and the caller angles the trajectory after
/// (`resolve_combat`).
pub fn strike<S: PunchableFace>(
    s: &mut S,
    sw: &Swing,
    contact: Vector2,
    t: &Tune,
) -> Option<Launch> {
    let blocked = match s.guard() {
        Guard::Open => false,
        Guard::Shield { .. } => true,
        Guard::Invuln | Guard::Intangible => return None,
    };
    let kb = knockback_units(s.percent() + sw.dmg, sw.dmg, s.heft(t), sw.hb) * sw.kb_scale;
    let speed = kb * t.kb_speed * t.knockback_mult;
    let l = Launch {
        dmg: sw.dmg,
        speed,
        vel: sw.aim.resolve(s.bound().0) * speed,
        hitstun: (kb * t.kb_hitstun) as i64,
        tumble: speed > t.tumble_speed,
        hitlag: (sw.dmg * HITLAG_PER_DMG) as i64 + sw.hitlag_bonus,
        interrupt: sw.interrupt,
        blocked,
    };
    s.absorb(l, contact, t);
    Some(l)
}

impl PunchableFace for Fighter {
    fn bound(&self) -> (Vector2, f32) {
        hurtbox(self)
    }
    fn hurt_shapes(&self, out: &mut impl FnMut(Iso, Shape)) {
        let (c, r) = hurtbox(self);
        out(Iso::at(c), Shape::Ball { r });
    }
    fn percent(&self) -> f32 {
        self.damage
    }
    fn heft(&self, t: &Tune) -> f32 {
        // Weight is a per-character stat: resolve it from THIS fighter's roster row, whichever
        // config the striking source happened to thread. Every resolved `Tune` carries the roster,
        // so item/ink/melee hits all pull the victim's own weight (bit-identical when rows match).
        t.weight_of(self.char_id)
    }
    fn guard(&self) -> Guard {
        if self.invuln > 0 {
            Guard::Invuln
        } else if self.intangible {
            Guard::Intangible
        } else if self.state == CharState::Shield {
            Guard::Shield {
                left: self.shield_hp,
            }
        } else {
            Guard::Open
        }
    }
    fn absorb(&mut self, l: Launch, _contact: Vector2, t: &Tune) {
        if l.blocked {
            // block path: shield hp eats the damage, guard stun locks the shield, the
            // launch collapses to a horizontal pushback slide. Empty shield = break.
            self.shield_hp -= l.dmg;
            self.shield_stun = (l.dmg * t.shieldstun_per_dmg) as i64 + 2;
            self.hitlag = l.hitlag;
            self.vel.x = sign(l.vel.x) * l.dmg * t.shield_push;
            if self.shield_hp <= 0.0 {
                self.shield_hp = 0.0;
                self.shield_stun = 0;
                self.state = CharState::ShieldBreak;
                self.frame = 0;
            }
            return;
        }
        self.damage += l.dmg;
        self.vel = l.vel;
        self.hitstun = l.hitstun;
        self.tumble = l.tumble;
        self.hitlag = l.hitlag;
        if l.interrupt {
            // hit interrupt: cancel whatever move the victim was mid-swing.
            // attack_for(Launched) is None, so remaining hit windows never fire.
            self.state = CharState::Launched;
            self.frame = 0;
        }
    }
}

// ── per-frame cross-fighter combat: hit resolution, clank, ink-as-hazard (R5: split out of
// lib.rs's `step`) ────────────────────────────────────────────────────────────────────────

/// Cross-fighter combat: `a`'s live hitboxes vs `b`'s hurtbox (circle/circle). `vb` is `b`'s slot
/// index (keys the per-box re-hit grid). Among `a`'s boxes live this frame, off cooldown for `b`,
/// and overlapping, the LOWEST id wins (sweetspot beats sourspot). On connect: damage + community/PM
/// knockback + hitstun to `b`, impact freeze (hitlag) to BOTH, and the hit forces `b` into
/// `Launched` (cancels any move `b` was mid-swing — the interrupt). `b` re-hittable per box per
/// `refresh`, so a 3-box jab combo / multi-hit stomp each land their own pops.
pub(crate) fn resolve_combat(
    a: &mut Fighter,
    vb: usize,
    b: &mut Fighter,
    b_aim: Vector2,
    ta: &Tune,
    tb: &Tune,
) {
    if b.invuln > 0 || b.intangible {
        return; // spawn i-frames / active dodge: no hit lands
    }
    let Some(atk) = attack_for(ta, a.state) else {
        return;
    };
    let (bc, br) = hurtbox(b);
    // id-priority pick: lowest-id live box that is off this victim's cooldown AND overlaps.
    let mut chosen: Option<usize> = None;
    let mut best_id = u8::MAX;
    for (bi, hb) in atk.live_boxes().iter().enumerate() {
        if !hb.live_at(a.frame) || a.hit_cd[bi][vb] > 0 {
            continue;
        }
        let (hc, hr) = hitbox_center(a, hb);
        if !geo::circles_touch(hc, hr, bc, br) {
            continue; // no overlap
        }
        if hb.id < best_id {
            best_id = hb.id;
            chosen = Some(bi);
        }
    }
    let Some(bi) = chosen else { return };
    let hb = atk.boxes[bi];
    // re-arm: a box can't re-hit this victim until `refresh` frames pass; with refresh 0 it locks for
    // the rest of its own window (one hit per box per swing). A later box (different index) still hits.
    let cd = if hb.refresh > 0 {
        hb.refresh
    } else {
        (hb.start + hb.len) - a.frame
    };
    a.hit_cd[bi][vb] = cd.max(1) as i16;

    let base = if is_aerial_attack(a.state) && a.autohop_aerial {
        hb.damage * ta.autohop_dmg // auto short-hop aerial: reduced damage (Ultimate)
    } else {
        hb.damage
    };
    let dmg = base * charge_mult(a, ta); // banked smash charge pays out here (kb follows dmg)
    // community / Project-M knockback via the shared ritual; interrupt launches the victim,
    // cancelling whatever move it was mid-swing (attack_for(Launched) is None).
    let Some(l) = strike(
        b,
        &Swing {
            hb: &hb,
            dmg,
            kb_scale: 1.0,
            hitlag_bonus: 4,
            interrupt: true,
            aim: Aim::Angle {
                deg: hb.angle,
                facing: a.facing,
            },
        },
        bc,
        tb, // victim's config: weight + shield-block stats come off b's character
    ) else {
        return;
    };
    if !l.blocked {
        b.vel = apply_di(b.vel, b_aim, tb.di_max_angle); // victim angles the trajectory (survival DI)
    }
    a.hitlag = l.hitlag; // both fighters pop on impact (blocked hits included)
}

/// Hitbox-vs-hitbox clank check for one fighter pair, run before hits resolve. The first live
/// non-transcendent box overlap decides it; damages compare charge-scaled (a charged smash
/// out-prioritizes what its raw damage would tie with).
pub(crate) fn resolve_clank(n: &mut SimState, a: usize, b: usize, ta: &Tune, tb: &Tune) {
    let (fa, fb) = (n.fighters[a], n.fighters[b]);
    if fa.hitlag > 0 || fb.hitlag > 0 {
        return;
    }
    let (Some(da), Some(db)) = (attack_for(ta, fa.state), attack_for(tb, fb.state)) else {
        return;
    };
    let mut met: Option<(f32, f32)> = None; // (a's box damage, b's box damage)
    'boxes: for ha in da.live_boxes() {
        if !ha.live_at(fa.frame) || ha.transcendent {
            continue;
        }
        for hb in db.live_boxes() {
            if !hb.live_at(fb.frame) || hb.transcendent {
                continue;
            }
            let (ca, ra) = hitbox_center(&fa, ha);
            let (cb, rb) = hitbox_center(&fb, hb);
            if geo::circles_touch(ca, ra, cb, rb) {
                met = Some((
                    ha.damage * charge_mult(&fa, ta),
                    hb.damage * charge_mult(&fb, tb),
                ));
                break 'boxes;
            }
        }
    }
    let Some((dmg_a, dmg_b)) = met else { return };
    let away = sign(fa.pos.x - fb.pos.x); // each staggers away from the other
    let diff = dmg_a - dmg_b;
    if diff.abs() <= ta.clank_diff {
        rebound(&mut n.fighters[a], away, ta);
        rebound(&mut n.fighters[b], -away, tb);
    } else if diff < 0.0 {
        rebound(&mut n.fighters[a], away, ta); // outdamaged: only the weaker move cancels
    } else {
        rebound(&mut n.fighters[b], -away, tb);
    }
}

/// Cancel a clanked move: brief stagger shoved `away` (+1 = right), then Stand (the Rebound arm).
fn rebound(f: &mut Fighter, away: f32, t: &Tune) {
    f.state = CharState::Rebound;
    f.frame = 0;
    f.vel.x = away * t.rebound_push;
}

/// Traveling ink slams fighters: any body moving at least `INK_TRUCK_SPEED` that overlaps a
/// hurtbox hits like a truck — no owner exemption, you CAN eat your own lob on the ricochet.
/// Mirrors `apply_bolt_hit`'s tail, but the launch direction is the body's travel direction
/// (up-biased) and damage scales with impact speed. Hitstun/hitlag/i-frames gate the re-hit:
/// a fighter already reeling can't be ground into paste by the same flyby.
// parity(v1-ink-fighter-hazard): a sufficiently fast traveling stroke hits any fighter including its owner through segment geometry and travel-directed launch
pub(crate) fn ink_hits_fighters(n: &mut SimState, np: usize, t: &Tune) {
    for pi in 0..stage::MAX_DRAWN {
        let p = n.paths[pi];
        if !p.traveling() {
            continue;
        }
        let sp = p.vel.length();
        if sp < stage::INK_TRUCK_SPEED {
            continue;
        }
        for f in n.fighters[..np].iter_mut() {
            if f.invuln > 0 || f.intangible || f.hitlag > 0 || f.hitstun > 0 {
                continue;
            }
            let (c, r) = hurtbox(f);
            let (bc, br) = p.bound_circle(&n.nodes);
            if !geo::circles_touch(c, r, bc, br) {
                continue;
            }
            let nseg = p.len as usize;
            let touched = (0..nseg.saturating_sub(1)).any(|s| {
                let (a, b) = p.world_seg(s, &n.nodes);
                (c - geo::closest_on_seg(c, a, b)).length() <= r
            });
            if !touched {
                continue;
            }
            // truck-grade box: heavy base + growth so it kills at high percent like a real hazard
            let hb = Hitbox {
                bkb: 50.0,
                kbg: 85.0,
                ..Hitbox::NONE
            };
            let dmg = (6.0 + sp * 0.5).min(18.0); // impact speed feeds the damage
            strike(
                f,
                &Swing {
                    hb: &hb,
                    dmg,
                    kb_scale: 1.0,
                    hitlag_bonus: 4,
                    interrupt: true,
                    aim: Aim::Carry {
                        vel: p.vel,
                        up: 0.5,
                    },
                },
                c,
                t,
            );
        }
    }
}
