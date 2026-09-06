// Split out of lib.rs; declared as a crate-root child so `super::*` still means the crate root.

use super::*;
use geo::{Geometry, NaiveGeom};

// Place an airborne fighter just left of the stage's left wall, in the wall band, moving INTO it.
fn at_left_wall(vel: Vector2, tumble: bool) -> SimState {
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.air_jumps = 0;
    f.pos = Vector2::new(FLOOR_LEFT - ECB_HALF_W + 4.0, GROUND_Y + 90.0); // overlaps face, cy in band
    f.vel = vel;
    f.tumble = tumble;
    s
}

// A launched (tumbling) body driven into the wall reflects back off it (geo::reflect, e>0).
#[test]
fn tumbling_body_bounces_off_wall() {
    let t = Tune::default();
    assert!(t.wall_bounce > 0.0);
    let s = at_left_wall(Vector2::new(900.0, 0.0), true);
    let idle = InputFrame::default();
    let out = step(&s, &[&idle, &idle], &t);
    let f = &out.fighters[0];
    assert!(
        f.vel.x < 0.0,
        "tumbling into the wall kicks back the other way, got {}",
        f.vel.x
    );
    assert!(
        (f.pos.x + ECB_HALF_W) <= FLOOR_LEFT + 1.0,
        "still depenetrated out of the wall"
    );
}

// A non-launched body dead-stops on the wall (reflect with e=0): no horizontal velocity left.
#[test]
fn neutral_body_dead_stops_on_wall() {
    let t = Tune::default();
    let s = at_left_wall(Vector2::new(900.0, 0.0), false);
    let idle = InputFrame::default();
    let out = step(&s, &[&idle, &idle], &t);
    assert!(
        out.fighters[0].vel.x.abs() < 1e-3,
        "no bounce without tumble (e=0 dead stop)"
    );
}

// The swept landing primitive rides the actual stage surface: a feet-circle falling onto the
// main platform's geo `platform_top` segment reports a forward time-of-impact with an upward
// normal. This proves the geo path matches the live bounding box landing surface (drawn-stage seam).
#[test]
fn swept_landing_rides_platform_top_segment() {
    let g = NaiveGeom;
    let top = platform_top(&PLATFORMS[0]); // solid main stage top at GROUND_Y
    let feet = (
        geo::Iso::at(Vector2::new(600.0, GROUND_Y - 100.0)),
        geo::Shape::Ball { r: 6.0 },
    );
    let hit = g
        .cast_shapes(feet, Vector2::new(0.0, 100.0), top, Vector2::ZERO, 2.0)
        .expect("feet falling onto the main stage must register a landing");
    assert!(hit.time_of_impact > 0.0 && hit.time_of_impact <= 2.0);
    // parry's `ShapeCastHit.normal1` is the OUTWARD normal on shape 1 (the falling feet ball), so a
    // downward landing points DOWN into the platform (+y) -- re-pinned from the retired hand-rolled
    // backend's surface-toward-mover (-y). No live caller reads cast_shapes' normal (the sweeps use
    // only `is_some()`); this pins the contract to the parry backend. See geo.rs `ShapeCastHit` doc.
    assert!(
        hit.normal1.y > 0.0,
        "parry normal1 is outward on the mover -> points down into the platform"
    );
}
