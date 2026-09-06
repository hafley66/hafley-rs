// The other half of the mover fixture's ride-carry/landing-inherit tests (mover_tests.rs):
// there the surface is a KINEMATIC fixture (mass 0, driven from `SimState.tick`). Here it's a
// live TRAVELING stroke (mass > 0, its own `vel` written by `integrate_ink`'s gravity solve) --
// plans/body-unify.md step 4's whole point is that both go through the SAME seam
// (`path_surface_vel`), so a fighter can ride/land on either without the consumer caring which.
// Declared as a crate-root child so `super::*` is the crate root, same as mover_tests.rs.

use super::*;
use crate::v1::arena::Scratch;

const IDLE: InputFrame = InputFrame {
    dir: 0.0,
    aim_y: 0.0,
    cx: 0.0,
    cy: 0.0,
    jump: false,
    jump_held: false,
    shorthop: false,
    shield_held: false,
    shield_pressed: false,
    down: false,
    down_pressed: false,
    attack: false,
    attack_held: false,
    grab: false,
    special: false,
};

/// A live traveling floor stroke (mass > 0, still under `integrate_ink`'s gravity, never
/// crossing another floor so it never settles) parked clear of every platform/fixture: centered
/// at `(170, 300)`, left of the left soft platform (starts at x=280) and ~422px from the ship's
/// bounding circle (center -190,520, radius 306) -- far past its cull reach. `drift` is the
/// horizontal vel under test (px/frame, ink-native); vertical vel starts at zero and grows a
/// hair from gravity each frame, same as any lobbed piece still in the air.
fn drifting_floor(tune: &Tune, drift: f32, nodes: &mut [InkNode], free: &mut FreeSpans) -> InkPath {
    let points = [Vector2::new(110.0, 300.0), Vector2::new(230.0, 300.0)];
    let mut path = rehydrate_stroke(&points, 0, 0, nodes, free, tune);
    path.vel = Vector2::new(drift, 0.0);
    path
}

// ── ride carry ──────────────────────────────────────────────────────────────

#[test]
fn fighter_rides_a_slow_traveling_stroke_alongside_it() {
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = drifting_floor(&tune, 1.5, &mut sim_state.nodes, &mut sim_state.free);
    assert!(
        sim_state.paths[slot].traveling(),
        "mass > 0 and vel != ZERO: a live traveling body, not a baked fixture"
    );
    let home = sim_state.paths[slot].pos;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Stand;
    fighter.ground_plat = 0; // the grounded-on-ink convention
    fighter.ground_ink = slot as i8;
    fighter.pos = home;
    fighter.vel = Vector2::ZERO;
    let mut stepped = sim_state;
    for _ in 0..20 {
        stepped = step(&stepped, &[&IDLE, &IDLE], &tune);
        let fighter = stepped.fighters[0];
        assert_eq!(
            fighter.ground_ink, slot as i8,
            "still riding at tick {}",
            stepped.tick
        );
        assert!(
            (fighter.pos.x - stepped.paths[slot].pos.x).abs() < 1.0,
            "x tracks the drifting stroke at tick {}: fighter {} vs stroke {}",
            stepped.tick,
            fighter.pos.x,
            stepped.paths[slot].pos.x
        );
    }
    // the stroke really did move -- otherwise the tracking assert above is vacuous.
    assert!(
        stepped.paths[slot].pos.x > home.x + 10.0,
        "the stroke actually drifted: {} vs home {}",
        stepped.paths[slot].pos.x,
        home.x
    );
}

// ── landing inherit ─────────────────────────────────────────────────────────

#[test]
fn landing_on_a_traveling_stroke_inherits_its_velocity() {
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = drifting_floor(&tune, 2.0, &mut sim_state.nodes, &mut sim_state.free);
    let home = sim_state.paths[slot].pos;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(home.x, home.y - 40.0); // straight above, falling
    fighter.vel = Vector2::new(0.0, 300.0);
    let mut stepped = sim_state;
    let mut landed = false;
    for _ in 0..30 {
        stepped = step(&stepped, &[&IDLE, &IDLE], &tune);
        let fighter = stepped.fighters[0];
        if fighter.ground_ink == slot as i8 {
            let surf_vel = stepped.paths[slot].vel.x * FPS; // the surf vel (px/s) this frame
            assert!(surf_vel > 0.0, "phase check: stroke still drifting right");
            assert!(
                (fighter.vel.x - surf_vel).abs() < 30.0,
                "landing frame vel.x includes the surf vel: {} vs {}",
                fighter.vel.x,
                surf_vel
            );
            landed = true;
            break;
        }
    }
    assert!(landed, "never landed on the traveling stroke");
}

// ── wall-cling ride carry: the cling freeze is RELATIVE to a moving wall ────────────────────────

#[test]
fn clinging_to_a_moving_wall_rides_its_translation() {
    // 2026-07-05 playtest: clinging to the flying hull froze the fighter in ABSOLUTE space while
    // the hull kept moving, so the wall slid out from under the cling and the fighter dropped.
    // Rig: a traveling vertical wall stroke, zero-g like the hull (only its drop-in vel moves
    // it), descending at a steady 1.2 px/frame; the fighter clings to its left face.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    // Clear airspace: x=600 sits between the soft platforms (280-540 / 660-920, y 575), above
    // everything, and outside the parked hull's bounding circle (center -190, radius 306).
    let points = [Vector2::new(600.0, 60.0), Vector2::new(600.0, 280.0)];
    let mut wall = rehydrate_stroke(
        &points,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    );
    wall.props.gravity_scale = 0.0;
    wall.vel = Vector2::new(0.0, 1.2);
    sim_state.paths[slot] = wall;
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, 170.0); // mid-span, left face
    fighter.vel = Vector2::new(400.0, 0.0); // moving into the wall
    let into = InputFrame { dir: 1.0, ..IDLE }; // held toward the wall the whole test: cling
    let mut c = step(&sim_state, &[&into, &IDLE], &tune);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    assert_eq!(
        c.fighters[0].wall_ink, slot as i8,
        "the armed contact records the wall's ink slot"
    );
    let y0 = c.fighters[0].pos.y;
    let frames = 30; // well under the cling budget and the wall's span
    for k in 1..=frames {
        c = step(&c, &[&into, &IDLE], &tune);
        assert!(c.fighters[0].wall_touch > 0, "still clung at frame {k}");
    }
    let carried = c.fighters[0].pos.y - y0;
    let wall_moved = frames as f32 * 1.2;
    assert!(
        (carried - wall_moved).abs() < 2.0,
        "the cling rides the descending wall: fighter carried {carried} vs wall {wall_moved}"
    );
}

#[test]
fn clinging_fighter_catches_up_to_a_walls_hard_bounce_off_the_top_blast_wall() {
    // plans/ship-containment.md row 4 direction A, the wall-cling branch: `cling_wall_vel`
    // already carries BOTH axes directly (`n.pos += wall_ride`, za_warudo.rs ~907, no overwrite
    // on either axis -- unlike the grounded-ink branch's y), so `real_delta - stale_carry` alone
    // is the whole correction here, no ground-branch asymmetry needed. Rig: a BAKED (owner < 0,
    // so it bounces off the invincible blast frame the same way the hull does) vertical wall,
    // zero-g, centered EXACTLY on `BLAST_TOP` (-520) so a -4.9 px/frame upward drift crosses the
    // wall this same tick and the bounce clamp cancels it back to net ZERO movement -- a clean,
    // sharp assertion: the wall doesn't move at all end-to-end despite a nonzero velocity, and
    // neither should a fighter clinging to it (pre-fix, the FSM's stale-snapshot carry would
    // still drag the fighter by the raw -4.9, drifting it away from a wall that never actually
    // moved).
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    let points = [Vector2::new(600.0, -520.0), Vector2::new(600.0, -300.0)];
    let mut wall = rehydrate_stroke(
        &points,
        0,
        -1,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    ); // owner < 0: baked, bounces off blast
    wall.props.gravity_scale = 0.0;
    sim_state.paths[slot] = wall; // vel ZERO for now -- the priming step arms the cling first
    assert_eq!(
        sim_state.paths[slot].pos.y, -410.0,
        "phase check: centroid sits at the span's midpoint"
    );
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, -410.0); // mid-span, left face
    fighter.vel = Vector2::new(400.0, 0.0); // moving into the wall
    let into = InputFrame { dir: 1.0, ..IDLE };
    // Priming step: establishes `wall_touch`/`wall_ink` (armed only AFTER the FSM's own cling
    // decision, so the very first contact frame never actually carries -- same reason
    // `clinging_to_a_moving_wall_rides_its_translation` above primes before measuring drift).
    let mut c = step(&sim_state, &[&into, &IDLE], &tune);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    assert_eq!(c.fighters[0].wall_ink, slot as i8, "clings to the wall");
    // NOW give the wall its bounce-triggering velocity and take the tick under test.
    c.paths[slot].vel = Vector2::new(0.0, -4.9);
    let before_wall_y = c.paths[slot].pos.y;
    let before_crew_y = c.fighters[0].pos.y;
    let c = step(&c, &[&into, &IDLE], &tune);
    assert!(
        c.fighters[0].wall_touch > 0,
        "still clung through the bounce tick"
    );
    assert_eq!(
        c.fighters[0].wall_ink, slot as i8,
        "clings to the bouncing wall"
    );
    let wall_dy = c.paths[slot].pos.y - before_wall_y;
    assert!(
        wall_dy.abs() < 0.1,
        "phase check: the bounce clamp (starting exactly at the boundary) cancels the wall's own \
         net displacement to zero: {wall_dy}"
    );
    let crew_dy = c.fighters[0].pos.y - before_crew_y;
    assert!(
        crew_dy.abs() < 0.1,
        "a clinging fighter must not drift off a wall that never actually moved: crew moved {crew_dy}"
    );
}

// ── wall dead-stop vs tumble bounce (drop-test playtest: "purple repells me, no wall hang") ──────

/// A static baked-material vertical wall stroke (PEN, so `restitution = 0.4`, the ink-billiard
/// row) parked in clear airspace at x=600, y 60..280 -- same clear slot the moving-wall cling test
/// uses. Zero-g + zero vel: it just sits there as a collision face, like the parked pillar.
fn static_pen_wall(tune: &Tune, nodes: &mut [InkNode], free: &mut FreeSpans) -> InkPath {
    let points = [Vector2::new(600.0, 60.0), Vector2::new(600.0, 280.0)];
    let mut wall = rehydrate_stroke(&points, 0, 0, nodes, free, tune);
    wall.props.gravity_scale = 0.0;
    wall.vel = Vector2::ZERO;
    wall
}

#[test]
fn wall_does_not_repel_a_falling_fighter() {
    // The bug: `StrokeProps::PEN.bounce = 0.4` leaked into fighter-vs-wall reflect, so a plain
    // airborne fighter drifting into a solid wall got a 40% rebound ("purple repells me") instead
    // of dead-stopping flush (which is what makes the cling/walljump window usable). Neutral input:
    // no cling, no walljump -- pure contact. Pre-fix vel.x reflects to ~-160 (kicked back away from
    // the wall); post-fix it dead-stops to ~0 and the fighter pins flush to the face.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = static_pen_wall(&tune, &mut sim_state.nodes, &mut sim_state.free);
    assert_eq!(
        sim_state.paths[slot].props.bounce, 0.4,
        "PEN wall carries the billiard restitution"
    );
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, 170.0); // mid-span, left face
    fighter.vel = Vector2::new(400.0, 200.0); // drifting right into the wall while falling
    let stepped = step(&sim_state, &[&IDLE, &IDLE], &tune);
    let after = stepped.fighters[0];
    assert!(
        after.vel.x.abs() < 25.0,
        "the wall dead-stops horizontal contact (no rebound): vel.x = {}",
        after.vel.x
    );
    assert!(
        after.vel.x > -25.0,
        "and it is NOT kicked back away from the wall (the 0.4 rebound): vel.x = {}",
        after.vel.x
    );
    assert!(
        (after.pos.x - (wall_x - ECB_HALF_W)).abs() < 2.0,
        "pos.x pinned flush to the wall face, not bounced off it: {} vs {}",
        after.pos.x,
        wall_x - ECB_HALF_W
    );
}

#[test]
fn holding_into_wall_clings() {
    // The hang the user wants: airborne contact + stick held INTO the wall + cling budget -> cling
    // engages (cling_used increments, wall_ink armed to the stroke slot, the fighter doesn't just
    // fall away). The repel was overriding this window; with the dead-stop it is usable.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = static_pen_wall(&tune, &mut sim_state.nodes, &mut sim_state.free);
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, 170.0);
    fighter.vel = Vector2::new(400.0, 0.0); // moving into the wall
    let into = InputFrame { dir: 1.0, ..IDLE }; // held toward the wall: cling
    // Priming step arms wall_touch/wall_ink (armed only AFTER the FSM's cling decision, so the very
    // first contact frame never carries -- same reason the moving-wall cling test primes first).
    let primed = step(&sim_state, &[&into, &IDLE], &tune);
    assert!(
        primed.fighters[0].wall_touch > 0,
        "wall contact arms the window"
    );
    assert_eq!(
        primed.fighters[0].wall_ink, slot as i8,
        "armed to the wall's ink slot"
    );
    let cling_before = primed.fighters[0].cling_used;
    // The tick under test: holding in with the window armed spends a cling frame.
    let clung = step(&primed, &[&into, &IDLE], &tune);
    assert!(
        clung.fighters[0].cling_used > cling_before,
        "holding into the wall spends a cling frame: {} -> {}",
        cling_before,
        clung.fighters[0].cling_used
    );
    assert_eq!(
        clung.fighters[0].wall_ink, slot as i8,
        "still armed to the wall it clings"
    );
    assert!(
        clung.fighters[0].wall_touch > 0,
        "still clung, did not fall away"
    );
}

#[test]
fn tumbling_body_still_bounces_off_wall() {
    // Regression guard: the dead-stop is for NON-tumble contact only. A launched (tumbling) fighter
    // into a wall must STILL rebound (wall_bounce / restitution, whichever is bigger) -- the
    // PM/Ultimate wall bounce. Launched routing (`hitstun > 0`) runs `hitstun_slide`, the sibling
    // reflect site.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = static_pen_wall(&tune, &mut sim_state.nodes, &mut sim_state.free);
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, 170.0);
    fighter.vel = Vector2::new(400.0, 0.0); // launched into the wall
    fighter.tumble = true;
    fighter.hitstun = 30; // > 0 routes to hitstun_slide (the launched knockback path)
    fighter.tech_buf = 0; // no tech armed: bounce, not wall-tech
    let stepped = step(&sim_state, &[&IDLE, &IDLE], &tune);
    let after = stepped.fighters[0];
    assert!(
        after.vel.x < -50.0,
        "a tumbling launch still rebounds off the wall (wall_bounce preserved): vel.x = {}",
        after.vel.x
    );
}

#[test]
fn wall_tech_still_fires() {
    // Regression guard: tumble + a buffered tech into a wall must still convert to CharState::TechWall
    // (intangible recovery in place). The dead-stop change must not flatten this.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    let slot = 0;
    sim_state.paths[slot] = static_pen_wall(&tune, &mut sim_state.nodes, &mut sim_state.free);
    let wall_x = sim_state.paths[slot].world_pt(0, &sim_state.nodes).x;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(wall_x - ECB_HALF_W - 4.0, 170.0);
    fighter.vel = Vector2::new(400.0, 0.0); // launched into the wall
    fighter.tumble = true;
    fighter.hitstun = 30; // launched routing
    fighter.tech_buf = 10; // tech armed at wall contact -> wall tech
    let stepped = step(&sim_state, &[&IDLE, &IDLE], &tune);
    assert_eq!(
        stepped.fighters[0].state,
        CharState::TechWall,
        "tumble + buffered tech into a wall techs it"
    );
}

// ── unpark the ship (plans/body-unify.md step 6): the ACTUAL hull, not a synthetic bar ──────────

#[test]
fn flying_hull_bounces_off_the_invincible_stage() {
    // Step 2's generic solve, exercised by the real baked hull: a restitution > 0 material (the
    // hull inherits PEN's `bounce: 0.4` unedited) rebounds off the main stage floor instead of
    // locking, same shape as `stage::tests::restitution_positive_bounces_off_the_baked_stage_then_bakes`.
    let t = Tune::default();
    let mut sc = Scratch::new();
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    let mut hull = bake_ship(&mut sc.nodes, &mut sc.free);
    assert_eq!(hull.props.bounce, 0.4, "unedited PEN restitution row");
    assert_eq!(
        hull.props.gravity_scale, 0.0,
        "zero-g: only the drop-in vel below moves it"
    );
    // reposition over the main stage floor (y=760, x in [150,1050]) and give it a falling vel --
    // no thrust involved, this test is purely about the stage-contact machinery.
    hull.pos = Vector2::new(400.0, 700.0);
    hull.vel = Vector2::new(0.0, 2.0);
    let mut bounced = false;
    let mut frames = 0;
    while hull.traveling() && frames < 2000 {
        integrate_ink(
            &mut hull,
            &empty,
            SHIP_SLOT,
            &mut sc.nodes,
            &mut sc.free,
            &t,
        );
        if hull.vel.y < 0.0 {
            bounced = true;
        }
        frames += 1;
    }
    assert!(
        bounced,
        "the flying hull rebounded off the stage before settling"
    );
}

#[test]
fn prune_exemption_keys_on_owner_not_stillness() {
    // The hull is `owner: -1` always (never expires); `prune_outside` must exempt it whether it's
    // parked OR mid-flight far outside the blast frame -- the exemption checks `owner`/mass, never
    // "is it still" (step 6 audit item).
    let mut s = SimState::spawn();
    s.paths[SHIP_SLOT].pos = Vector2::new(BLAST_RIGHT + 5000.0, BLAST_Y + 5000.0);
    s.paths[SHIP_SLOT].vel = Vector2::new(50.0, 50.0); // traveling, well outside every blast edge
    assert!(s.paths[SHIP_SLOT].traveling(), "moving, not parked");
    prune_outside(&mut s);
    assert!(
        s.paths[SHIP_SLOT].active(),
        "the hull survives prune while flying, same as parked"
    );
}

#[test]
fn a_launched_hull_survives_crossing_the_blast_floor() {
    // Companion to the prune exemption above, but for `integrate_ink`'s own BLAST_Y wipe: giving
    // the hull mass makes it strikeable (`PunchableFace::guard` reads `mass > 0` as Open), so a
    // hard enough hit could otherwise send it past this line and delete it outright, permanently,
    // with no respawn -- unlike a fighter. `owner < 0` survives AND (2026-07-04, the hard-red-
    // walls call) BOUNCES: the blast frame is a wall for baked bodies, so the hull reflects
    // back inside instead of sailing off.
    let t = Tune::default();
    let mut sc = Scratch::new();
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    let mut hull = bake_ship(&mut sc.nodes, &mut sc.free);
    hull.pos = Vector2::new(SHIP_HOME.x, BLAST_Y - 5.0);
    hull.vel = Vector2::new(0.0, 50.0); // native px/frame, well clear of BLAST_Y next step
    integrate_ink(
        &mut hull,
        &empty,
        SHIP_SLOT,
        &mut sc.nodes,
        &mut sc.free,
        &t,
    );
    assert!(
        hull.active(),
        "the hull survives meeting the blast floor, unlike ordinary ink"
    );
    let (center, radius) = hull.bound_circle(&sc.nodes);
    assert!(
        center.y + radius <= BLAST_Y + 1.0,
        "the blast floor is a hard wall: the hull is clamped back inside, bottom at {}",
        center.y + radius
    );
    assert!(
        hull.vel.y < 0.0,
        "and its velocity reflected off the wall, vel.y={}",
        hull.vel.y
    );

    // control: an ordinary (owner >= 0) traveling stroke IS wiped crossing the same line --
    // pins that the exemption is scoped to owner, not a change to the wipe itself.
    let mut ordinary = bake_ship(&mut sc.nodes, &mut sc.free);
    ordinary.owner = 0;
    ordinary.pos = Vector2::new(SHIP_HOME.x, BLAST_Y - 5.0);
    ordinary.vel = Vector2::new(0.0, 50.0);
    integrate_ink(
        &mut ordinary,
        &empty,
        SHIP_SLOT,
        &mut sc.nodes,
        &mut sc.free,
        &t,
    );
    assert!(
        !ordinary.active(),
        "an owned stroke still dies crossing the blast floor"
    );
}
