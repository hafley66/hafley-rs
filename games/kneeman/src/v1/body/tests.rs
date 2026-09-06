//! Unit tests for the terrain bus core: `collide` (rigid impulse exchange), `sweep_floors`,
//! `sweep_walls`, the one-way gate predicate, and the wall-deflect helpers. Split out of
//! `body/mod.rs` (own file, own `use super::*`) to buy line-budget headroom for the row-5
//! hull-vs-stage-ink containment scope (plans/ship-containment.md §4), same reasoning
//! `gate_tests.rs` already used for the lateral gate sweep tests -- both reuse the
//! `floor`/`wall`/`gated` fixture builders defined here (`pub(super)` for exactly that).

use super::*;

pub(super) fn floor(ax: f32, ay: f32, bx: f32, by: f32, solid: bool) -> Surf {
    let (a, b) = (Vector2::new(ax, ay), Vector2::new(bx, by));
    Surf {
        a,
        b,
        solid,
        vel: Vector2::ZERO,
        kind: SurfKind::Floor,
        owner: SurfOwner::Platform(0),
        gate: if solid {
            SideGate::Solid
        } else {
            SideGate::Soft
        },
        gate_normal: segment_gate_normal(a, b),
        restitution: 0.0,
    }
}
pub(super) fn wall(x: f32, top: f32, bot: f32) -> Surf {
    let (a, b) = (Vector2::new(x, top), Vector2::new(x, bot));
    Surf {
        a,
        b,
        solid: true,
        vel: Vector2::ZERO,
        kind: SurfKind::Wall,
        owner: SurfOwner::Platform(0),
        gate: SideGate::Solid,
        gate_normal: segment_gate_normal(a, b),
        restitution: 0.0,
    }
}
/// Same surf, a different gate -- the one-way tests below only ever vary this.
pub(super) fn gated(mut s: Surf, gate: SideGate) -> Surf {
    s.gate = gate;
    s
}

fn bits(pos: Vector2, vel: Vector2, mass: f32, inertia: f32) -> BodyBits {
    BodyBits {
        pos,
        vel,
        omega: 0.0,
        inv_mass: 1.0 / mass,
        inv_inertia: 1.0 / inertia,
    }
}

#[test]
fn head_on_equal_mass_elastic_swaps_velocities() {
    let mut a = bits(Vector2::new(0.0, 0.0), Vector2::new(10.0, 0.0), 2.0, 5.0);
    let mut b = bits(Vector2::new(10.0, 0.0), Vector2::new(0.0, 0.0), 2.0, 5.0);
    collide(
        &mut a,
        &mut b,
        Vector2::new(5.0, 0.0),
        Vector2::new(1.0, 0.0),
        1.0,
        0.0,
    );
    assert!((a.vel.x - 0.0).abs() < 1e-4 && (b.vel.x - 10.0).abs() < 1e-4);
    assert_eq!((a.omega, b.omega), (0.0, 0.0)); // through both centers: no torque
}

#[test]
fn momentum_is_conserved_off_center_and_spin_appears() {
    let mut a = bits(Vector2::new(0.0, 0.0), Vector2::new(8.0, 0.0), 3.0, 12.0);
    let mut b = bits(Vector2::new(10.0, 4.0), Vector2::ZERO, 5.0, 30.0);
    let before = a.vel * 3.0 + b.vel * 5.0;
    // contact above b's center: the normal impulse torques it
    collide(
        &mut a,
        &mut b,
        Vector2::new(9.0, 0.0),
        Vector2::new(1.0, 0.0),
        0.6,
        0.4,
    );
    let after = a.vel * 3.0 + b.vel * 5.0;
    assert!(
        (before - after).length() < 1e-3,
        "linear momentum conserved"
    );
    assert!(
        b.omega.abs() > 1e-4,
        "off-center contact spins the receiver"
    );
}

#[test]
fn immovable_body_reflects_the_traveler() {
    let mut a = bits(Vector2::new(0.0, 0.0), Vector2::new(6.0, 0.0), 2.0, 5.0);
    let mut wall = BodyBits {
        pos: Vector2::new(8.0, 0.0),
        vel: Vector2::ZERO,
        omega: 0.0,
        inv_mass: 0.0,
        inv_inertia: 0.0,
    };
    collide(
        &mut a,
        &mut wall,
        Vector2::new(4.0, 0.0),
        Vector2::new(1.0, 0.0),
        0.5,
        0.0,
    );
    assert!((a.vel.x + 3.0).abs() < 1e-4, "bounces back at e * speed");
    assert_eq!(wall.vel, Vector2::ZERO, "immovable stays put");
}

#[test]
fn separating_bodies_are_untouched() {
    let mut a = bits(Vector2::new(0.0, 0.0), Vector2::new(-5.0, 0.0), 2.0, 5.0);
    let mut b = bits(Vector2::new(10.0, 0.0), Vector2::new(5.0, 0.0), 2.0, 5.0);
    collide(
        &mut a,
        &mut b,
        Vector2::new(5.0, 0.0),
        Vector2::new(1.0, 0.0),
        1.0,
        0.5,
    );
    assert_eq!(a.vel, Vector2::new(-5.0, 0.0));
    assert_eq!(b.vel, Vector2::new(5.0, 0.0));
}

#[test]
fn sweep_floors_picks_highest_crossed() {
    let surfs = [
        floor(0.0, 100.0, 50.0, 100.0, true),
        floor(0.0, 80.0, 50.0, 80.0, false),
    ];
    // fell from y=70 to y=110: crossed both; the higher (y=80) catches first.
    let hit = sweep_floors(
        Vector2::new(10.0, 70.0),
        Vector2::new(10.0, 110.0),
        false,
        &surfs,
    )
    .unwrap();
    assert_eq!(hit.y, 80.0);
    // holding down: the soft one is skipped, the solid one catches.
    let hit = sweep_floors(
        Vector2::new(10.0, 70.0),
        Vector2::new(10.0, 110.0),
        true,
        &surfs,
    )
    .unwrap();
    assert_eq!(hit.y, 100.0);
}

#[test]
fn sweep_floors_interpolates_slopes_and_respects_span() {
    let surfs = [floor(0.0, 100.0, 10.0, 90.0, true)];
    let hit = sweep_floors(
        Vector2::new(5.0, 80.0),
        Vector2::new(5.0, 120.0),
        false,
        &surfs,
    )
    .unwrap();
    assert!((hit.y - 95.0).abs() < 1e-4);
    assert!(
        sweep_floors(
            Vector2::new(11.0, 80.0),
            Vector2::new(11.0, 120.0),
            false,
            &surfs
        )
        .is_none()
    );
}

#[test]
fn sweep_walls_blocks_catches_tunnel_and_shoves_overlap() {
    let surfs = [wall(100.0, 0.0, 200.0)];
    let (hw, hh) = (8.0, 20.0);
    // approach from the left, ECB right vert crosses the wall
    let h = sweep_walls(80.0, Vector2::new(95.0, 100.0), hw, hh, &surfs).unwrap();
    assert_eq!((h.x, h.nx), (92.0, -1.0));
    // full tunnel in one frame is still caught
    let h = sweep_walls(80.0, Vector2::new(130.0, 100.0), hw, hh, &surfs).unwrap();
    assert_eq!(h.nx, -1.0);
    // already overlapping: shoved back toward the side we came from
    let h = sweep_walls(120.0, Vector2::new(101.0, 100.0), hw, hh, &surfs).unwrap();
    assert_eq!((h.x, h.nx), (108.0, 1.0));
    // clear of the wall's span entirely (the OLD fixture here, pos.y=10.0, only looked clear
    // by the retired center-only sample: with hh=20.0 its box was y in [-30, 10], which actually
    // DIPS 10px into the wall's [0, 200] span -- the same shape as the 2026-07-07 stage-wall
    // tunneling bug, just smaller. `sweep_walls` now gates on the box's full extent, so this
    // fixture is moved to y=-50.0, whose box (y in [-90, -50]) is genuinely disjoint from [0, 200].
    assert!(sweep_walls(80.0, Vector2::new(95.0, -50.0), hw, hh, &surfs).is_none());
}

// ── one-way ink (plans/body-unify.md step 5) ──────────────────────────────────────────────

#[test]
fn sweep_floors_one_way_admits_the_pass_direction_and_blocks_the_other() {
    // A flat rightward floor's own tangent.perp() is DOWN (screen convention): forward=true
    // makes DOWN the pass direction, so a falling body (traveling DOWN) is admitted -- passes
    // clean through, no hit at all, just like there's no floor there.
    let passes = gated(
        floor(0.0, 100.0, 50.0, 100.0, true),
        SideGate::OneWay { forward: true },
    );
    assert!(
        sweep_floors(
            Vector2::new(10.0, 70.0),
            Vector2::new(10.0, 110.0),
            false,
            &[passes]
        )
        .is_none(),
        "falling with the pass direction must pass clean through"
    );
    // forward=false flips the oriented normal to UP: the same falling body now travels
    // AGAINST it, so the gate blocks -- an ordinary floor catch, same y as Solid.
    let blocks = gated(
        floor(0.0, 100.0, 50.0, 100.0, true),
        SideGate::OneWay { forward: false },
    );
    let hit = sweep_floors(
        Vector2::new(10.0, 70.0),
        Vector2::new(10.0, 110.0),
        false,
        &[blocks],
    )
    .expect("against the pass direction: caught like a floor");
    assert_eq!(hit.y, 100.0);
}

#[test]
fn sweep_walls_one_way_admits_the_pass_direction_and_blocks_the_other() {
    let (hw, hh) = (8.0, 20.0);
    // approaching from the left (rightward travel, the existing control's geometry):
    // forward=false makes rightward the pass direction here -- admitted, no WallHit at all.
    let passes = gated(wall(100.0, 0.0, 200.0), SideGate::OneWay { forward: false });
    assert!(
        sweep_walls(80.0, Vector2::new(95.0, 100.0), hw, hh, &[passes]).is_none(),
        "moving with the pass direction must pass clean through the wall"
    );
    // forward=true flips it: the same rightward approach is now AGAINST the pass direction,
    // so it blocks -- identical catch to the plain Solid wall test above.
    let blocks = gated(wall(100.0, 0.0, 200.0), SideGate::OneWay { forward: true });
    let h = sweep_walls(80.0, Vector2::new(95.0, 100.0), hw, hh, &[blocks])
        .expect("against the pass direction: blocked like a Solid wall");
    assert_eq!((h.x, h.nx), (92.0, -1.0));
}

#[test]
fn one_way_gate_never_changes_solid_or_soft_behavior() {
    // (c) Solid unchanged, (d) Soft/drop-through unchanged: gate_admits never admits either,
    // regardless of travel direction -- the two existing surfs' `gate` values (Solid from
    // `wall()`, Soft from `floor(.., false)`) already exercise this on every frame the
    // suite runs; this test just pins the predicate itself against both travel directions.
    for travel in [
        geo::DOWN,
        -geo::DOWN,
        Vector2::new(1.0, 0.0),
        Vector2::new(-1.0, 0.0),
    ] {
        let n = Vector2::new(0.0, 1.0);
        assert!(!gate_admits(SideGate::Solid, n, travel));
        assert!(!gate_admits(SideGate::Soft, n, travel));
    }
}

#[test]
fn wall_deflect_away_and_into_read_the_outward_normal() {
    // wall on the left (nx = +1, kick right): pushing right is AWAY, left is INTO.
    assert!(wall_deflect_away(1.0, 1.0, 1.0));
    assert!(!wall_deflect_away(1.0, -1.0, 1.0));
    assert!(wall_deflect_into(1.0, -1.0, 1.0));
    assert!(!wall_deflect_into(1.0, 1.0, 1.0));
    // wall on the right (nx = -1, kick left): signs flip.
    assert!(wall_deflect_away(-1.0, -1.0, 1.0));
    assert!(wall_deflect_into(-1.0, 1.0, 1.0));
    // under threshold: neither fires even in the right direction.
    assert!(!wall_deflect_away(1.0, 1.0, WALK_THRESH - 0.01));
    assert!(!wall_deflect_into(1.0, -1.0, WALK_THRESH - 0.01));
    // no armed wall (nx = 0): neither ever fires.
    assert!(!wall_deflect_away(0.0, 1.0, 1.0));
    assert!(!wall_deflect_into(0.0, -1.0, 1.0));
}
