//! Free-item floor contact (plans/body-unify.md step 3: "free items join the solve").
//! Thrown/unowned items route their crossed-floor contact through a generic
//! reflect-then-override shape -- the same shape `plans/body-unify.md` describes for
//! `integrate_ink`'s stage bounce -- instead of a hardcoded settle branch. `item_landing`
//! (item.rs) stays the one shared crossed-from-above sweep; this module is what a caller
//! does with the `FloorHit` it returns.
//!
//! Shares `body::contact::Material` with the ink lane (steps 1-2): one material row
//! type for every body class, per-kind rows lowered here. (This step was built on a
//! pre-steps-1-2 base with its own local row struct; unified onto `contact::Material`
//! at merge.)
//!
//! `Land::Detonate` (Bomb/Rocket) and `Land::Ignore` (bolts) are unchanged by this step:
//! they never reach `resolve_floor_contact` -- Detonate's landing check in
//! `items::behavior::blast_round_tick` still converts straight to `ItemFx::Explode`, and
//! Ignore kinds never produce a `FloorHit` at all (`item_landing`'s own early-out). Only
//! `Land::Settle` kinds (every held tool + badge) are rewired here.

use crate::v1::body::FloorHit;
use crate::v1::body::contact::{Material, SideGate};
use crate::v1::geo;
use crate::v1::{ItemKind, Vector2};

/// Below this rebound speed (px/s -- Item's native unit) the generic solve's bounce
/// reads as at-rest, so the settle override bakes the item into a resting ground
/// pickup. A restitution-0 row's rebound is always exactly 0, so this fires on the very
/// first contact frame no matter the approach speed -- byte-identical to the old
/// unconditional `vel = Vector2::ZERO` branch it replaces. A restitution>0 row (the pen)
/// keeps hopping until its rebound decays under this floor.
pub(crate) const ITEM_LOCK_REBOUND: f32 = 30.0;
/// Sideways speed kept through a bounce (ground friction of the hop) -- ink's own
/// `INK_BOUNCE_FRICTION` plays the same role for its body class; items get their own
/// row since the two are separate tuning surfaces.
const ITEM_BOUNCE_FRICTION: f32 = 0.8;

/// Per-kind free-item bounce/friction row (plans/body-unify.md step 3). `Land::Settle`
/// kinds default to restitution 0 -- their floor contact still runs the generic solve,
/// but restitution 0 collapses it to the old hardcoded settle exactly. The pen is the
/// one row picked to plausibly bounce (a dropped or thrown pen skips once before it
/// rests); everything else stays a dead stop until a future kind's row asks for more.
/// The gate is the SURFACE side's policy, already resolved by `item_landing`'s shared
/// sweep before this row is ever read -- items are the moving body here, so their own
/// gate is always `Solid`.
pub(crate) fn item_material(kind: ItemKind) -> Material {
    match kind {
        ItemKind::Pen => Material {
            restitution: 0.2,
            friction: ITEM_BOUNCE_FRICTION,
            gate: SideGate::Solid,
        },
        _ => Material {
            restitution: 0.0,
            friction: ITEM_BOUNCE_FRICTION,
            gate: SideGate::Solid,
        },
    }
}

/// One frame of a free item's crossed-floor contact (plans/body-unify.md step 3):
/// generic solve, then an at-rest override predicate -- `pos`/`vel` are the item's
/// post-gravity position/velocity for this frame (the caller's own integration, already
/// computed); `hit` is the floor `item_landing` returned. Returns the resolved
/// `(pos, vel)` and whether the contact settled (the override fired).
///
/// Always runs the GENERIC SOLVE first: normal reflection off the crossed floor at the
/// row's restitution, the surface as infinite mass (a fixed reflector, the same shape
/// `collide()` gives against `inv_mass` 0). The OVERRIDE PREDICATE (settle) then reads
/// the solved rebound speed: done rebounding -> lock to the floor as a resting ground
/// pickup (`vel` hard-zeroed, matching the old branch -- items don't yet inherit rider
/// `Surf.vel`, that is a later arc step).
pub(crate) fn resolve_floor_contact(
    kind: ItemKind,
    pos: Vector2,
    vel: Vector2,
    hit: &FloorHit,
) -> (Vector2, Vector2, bool) {
    let up = -geo::DOWN;
    let along = up.perp();
    let material = item_material(kind);

    // approach speed into the floor, captured before the solve -- a projection so this
    // stays dimension-disciplined: normals and dot products, never component cases.
    let approach_speed = vel.dot(geo::DOWN);

    // GENERIC SOLVE (always runs, never bypassed).
    let reflected = geo::reflect(vel, up, material.restitution);
    let along_speed = reflected.dot(along);
    let normal_vel = reflected - along * along_speed;
    let solved_vel = normal_vel + along * (along_speed * material.friction);

    // OVERRIDE PREDICATE (settle), applied after the solve.
    let rebound_speed = material.restitution * approach_speed;
    let settled = rebound_speed <= ITEM_LOCK_REBOUND;

    let resting_pos = Vector2::new(pos.x, hit.y);
    if settled {
        (resting_pos, Vector2::ZERO, true)
    } else {
        (resting_pos, solved_vel, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::body::SurfOwner;

    fn hit(y: f32) -> FloorHit {
        FloorHit {
            y,
            x: 0.0,
            vel: Vector2::ZERO,
            owner: SurfOwner::Platform(0),
        }
    }

    #[test]
    fn settle_kind_locks_dead_stop_on_first_contact_regardless_of_speed() {
        for approach in [10.0, 400.0, 4000.0] {
            let pos = Vector2::new(12.0, 100.0);
            let vel = Vector2::new(-30.0, approach);
            let (out_pos, out_vel, settled) =
                resolve_floor_contact(ItemKind::LaserGun, pos, vel, &hit(100.0));
            assert!(settled, "restitution-0 row always settles on first contact");
            assert_eq!(out_vel, Vector2::ZERO, "dead stop, same as the old branch");
            assert_eq!(out_pos, Vector2::new(12.0, 100.0));
        }
    }

    #[test]
    fn pen_bounces_before_it_settles_on_a_hard_impact() {
        let pos = Vector2::new(0.0, 100.0);
        let vel = Vector2::new(50.0, 900.0); // a hard fall, well above ITEM_LOCK_REBOUND once scaled
        let (_, out_vel, settled) = resolve_floor_contact(ItemKind::Pen, pos, vel, &hit(100.0));
        assert!(!settled, "a fast enough impact should still be rebounding");
        assert!(
            out_vel.y < 0.0,
            "the solved bounce moves back up (away from DOWN): got {out_vel:?}"
        );
    }

    #[test]
    fn pen_eventually_settles_as_its_rebound_decays() {
        // repeatedly re-enter the same floor contact with the shrinking rebound velocity,
        // the same way item.rs re-calls this every frame the item is still falling/bouncing
        // (gravity brings it back down at whatever speed the previous bounce left it).
        let mut vel = Vector2::new(50.0, 900.0);
        let mut settled = false;
        for _ in 0..50 {
            if settled {
                break;
            }
            let pos = Vector2::new(0.0, 100.0);
            let (_, out_vel, s) = resolve_floor_contact(ItemKind::Pen, pos, vel, &hit(100.0));
            settled = s;
            vel = Vector2::new(out_vel.x, -out_vel.y);
        }
        assert!(
            settled,
            "a restitution < 1 rebound must decay to the lock threshold"
        );
    }

    #[test]
    fn resting_pos_locks_to_the_floor_y_keeping_x() {
        let (out_pos, _, _) = resolve_floor_contact(
            ItemKind::BobGun,
            Vector2::new(42.0, 210.0),
            Vector2::new(5.0, 300.0),
            &hit(200.0),
        );
        assert_eq!(out_pos, Vector2::new(42.0, 200.0));
    }
}
