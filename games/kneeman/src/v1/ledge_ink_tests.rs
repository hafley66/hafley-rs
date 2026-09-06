//! Ledges are ink command grabs (plans/ledge-domain.md, directed 2026-07-05): a Ledge-classed
//! Floor tip on ANY active stroke is a grabbable lip, derived per frame (LipSoup, zero stored
//! bytes) alongside the two hardcoded stage lips, and the catch is a hand-anchor radius test.
//! These pin: (a) a drawn sharp tip classifies Ledge and a falling fighter hand-grabs it;
//! (b) a hang on a MOVING zero-g stroke rides the stroke's translation; (c) a severed lip drops
//! the hang to Air; (d) the stage lips still snap + hang + climb. Crate-root child (`super::*`).

use super::*;

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

/// An L-shaped stroke in clear airspace (x~560-680, y~200-320, above the platforms and clear of
/// the parked hull's bounding circle): a flat floor tip that drops off into a wall. The corner at
/// node 1 (and the flat segment's open start) classifies `Ledge`. Slot 0, `active = 1` so only
/// fighter 0 steps.
fn l_stroke_scene(tune: &Tune) -> (SimState, usize) {
    let points = [
        Vector2::new(560.0, 200.0),
        Vector2::new(680.0, 200.0),
        Vector2::new(680.0, 320.0),
    ];
    let mut sim_state = SimState::spawn();
    sim_state.active = 1;
    let slot = 0;
    sim_state.paths[slot] = rehydrate_stroke(
        &points,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        tune,
    );
    (sim_state, slot)
}

#[test]
fn drawn_sharp_tip_classifies_ledge_and_a_falling_fighter_grabs_it() {
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let (mut sim_state, slot) = l_stroke_scene(&tune);
    // the flat floor segment (node 0 -> node 1) turns hard into the wall at node 1: a Ledge.
    assert_eq!(
        sim_state.paths[slot].seg_class(0, &sim_state.nodes),
        SegClass::Ledge,
        "the sharp floor tip classifies as a grabbable Ledge"
    );
    // fighter falling just outboard of the right lip (node 1 at ~680,200), within hand reach.
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(712.0, 235.0);
    fighter.vel = Vector2::new(0.0, 400.0); // falling past the fall threshold
    let after = step(&sim_state, &[&IDLE, &IDLE], &tune);
    let fighter = after.fighters[0];
    assert_eq!(
        fighter.state,
        CharState::LedgeHold,
        "hand-grabbed the ink lip"
    );
    assert_eq!(fighter.ledge_ink, slot as i8, "hang remembers its ink slot");
    assert_eq!(
        fighter.facing, -1.0,
        "faces inward (toward the floor it grabbed)"
    );
    assert_eq!(fighter.vel, Vector2::ZERO, "hanging still");
}

#[test]
fn a_hang_on_a_moving_stroke_rides_its_translation() {
    // Rig copied from surf_vel_tests::clinging_to_a_moving_wall_rides_its_translation: a zero-g
    // (`gravity_scale = 0`) stroke whose only motion is its drop-in vel, descending at a steady
    // 1.2 px/frame. A hang on its lip must re-pin to the lip's CURRENT world point each frame and
    // ride the descent, never freeze in absolute space (the moving-body trap).
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    sim_state.active = 1;
    let slot = 0;
    // a flat floor bar; its right open end (node 1 at ~680,200) is a lip.
    let points = [Vector2::new(560.0, 200.0), Vector2::new(680.0, 200.0)];
    let mut bar = rehydrate_stroke(
        &points,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    );
    bar.props.gravity_scale = 0.0;
    bar.vel = Vector2::new(0.0, 1.2);
    sim_state.paths[slot] = bar;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(712.0, 235.0);
    fighter.vel = Vector2::new(0.0, 400.0);
    let mut current = step(&sim_state, &[&IDLE, &IDLE], &tune);
    assert_eq!(
        current.fighters[0].state,
        CharState::LedgeHold,
        "caught the moving stroke's lip"
    );
    assert_eq!(current.fighters[0].ledge_ink, slot as i8);
    let y0 = current.fighters[0].pos.y;
    let frames = 30; // under the stroke's descent to any floor
    for _ in 0..frames {
        current = step(&current, &[&IDLE, &IDLE], &tune);
        assert_eq!(
            current.fighters[0].state,
            CharState::LedgeHold,
            "still hanging as the stroke moves"
        );
    }
    let carried = current.fighters[0].pos.y - y0;
    let stroke_moved = frames as f32 * 1.2;
    assert!(
        (carried - stroke_moved).abs() < 2.0,
        "the hang rides the descending stroke: carried {carried} vs stroke {stroke_moved}"
    );
}

#[test]
fn a_severed_lip_drops_the_hang_to_air() {
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    sim_state.active = 1;
    let slot = 0;
    let points = [Vector2::new(560.0, 200.0), Vector2::new(680.0, 200.0)];
    sim_state.paths[slot] = rehydrate_stroke(
        &points,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    );
    // hang the fighter on the right lip (node 1) directly.
    let lip = sim_state.paths[slot].world_pt(1, &sim_state.nodes);
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::LedgeHold;
    fighter.pos = lip + Vector2::new(0.0, 44.0);
    fighter.vel = Vector2::ZERO;
    fighter.facing = -1.0;
    fighter.frame = 40; // i-frames spent
    fighter.ledge_ink = slot as i8;
    fighter.ledge_node = 1;
    // sever the path: the lip no longer exists.
    sim_state.paths[slot] = InkPath::EMPTY;
    let after = step(&sim_state, &[&IDLE, &IDLE], &tune);
    let fighter = after.fighters[0];
    assert_eq!(
        fighter.state,
        CharState::Air,
        "the severed lip dropped the hang"
    );
    assert_eq!(
        fighter.ledge_ink, -1,
        "the stale ink ref is cleared on the drop"
    );
}

#[test]
fn climb_from_an_ink_hang_plants_feet_on_the_stroke() {
    // Deliverable #3: climb must work from an ink hang -- feet onto the lip node, grounded on the
    // stroke (not teleported to the main stage floor).
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let (mut sim_state, slot) = l_stroke_scene(&tune);
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(712.0, 235.0);
    fighter.vel = Vector2::new(0.0, 400.0);
    let mut current = step(&sim_state, &[&IDLE, &IDLE], &tune);
    assert_eq!(
        current.fighters[0].state,
        CharState::LedgeHold,
        "grabbed the ink lip"
    );
    // hold inward (facing was set to -1 at the grab): neutral getup climb.
    let toward = InputFrame { dir: -1.0, ..IDLE };
    current = step(&current, &[&toward, &IDLE], &tune);
    assert_eq!(
        current.fighters[0].state,
        CharState::LedgeClimb,
        "climb started off the ink lip"
    );
    for _ in 0..(tune.climb_frames + 2) {
        current = step(&current, &[&IDLE, &IDLE], &tune);
    }
    let fighter = current.fighters[0];
    assert_eq!(fighter.state, CharState::Stand, "climbed onto the stroke");
    assert_eq!(
        fighter.ground_ink, slot as i8,
        "grounded ON the ink, not the main stage"
    );
    assert_eq!(fighter.ledge_ink, -1, "no longer hanging");
    assert!(
        fighter.pos.x < 680.0 && fighter.pos.x > 600.0,
        "planted just inside the lip on the floor span, x={}",
        fighter.pos.x
    );
}

#[test]
fn stage_lip_snaps_hangs_and_climbs() {
    // The two hardcoded stage lips survive the LipSoup rewrite (owner -1): the same catch ritual,
    // the same fixed hang, the same climb onto the stage floor.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    sim_state.active = 1;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(FLOOR_RIGHT + 30.0, GROUND_Y + 10.0);
    fighter.vel = Vector2::new(0.0, 400.0);
    let mut current = step(&sim_state, &[&IDLE, &IDLE], &tune);
    let fighter = current.fighters[0];
    assert_eq!(fighter.state, CharState::LedgeHold, "snapped the stage lip");
    assert_eq!(fighter.ledge_ink, -1, "a stage lip is not an ink lip");
    assert_eq!(fighter.facing, -1.0, "faces the stage");
    // hold toward the stage (facing -1): neutral getup climb -> onto the floor.
    let toward = InputFrame { dir: -1.0, ..IDLE };
    current = step(&current, &[&toward, &IDLE], &tune);
    assert_eq!(
        current.fighters[0].state,
        CharState::LedgeClimb,
        "climb started"
    );
    for _ in 0..(tune.climb_frames + 2) {
        current = step(&current, &[&IDLE, &IDLE], &tune);
    }
    let fighter = current.fighters[0];
    assert_eq!(fighter.state, CharState::Stand, "climbed onto the stage");
    assert_eq!(fighter.ground_plat, 0, "standing on the main floor");
    assert_eq!(
        fighter.pos.x,
        FLOOR_RIGHT - 30.0,
        "planted just inside the lip"
    );
}

// ── plans/ledge-ship-fixes.md #1: directional below+outboard grab zone ─────────────────────────────

#[test]
fn body_above_the_lip_falling_fast_does_not_grab_the_turbo_glue_regression() {
    // Root cause (ledge-ship-fixes.md #0): the old catch had no Y gate at all, so a body ABOVE the
    // lip, outboard in x, falling fast, caught INSTANTLY -- the "turbo glue." The directional box's
    // ceiling term (`ledge_ceil`) rejects it while still above; only once it has fallen into the
    // below+outboard band does it catch (the baseline behavior stays intact).
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let (sim_state, _slot) = l_stroke_scene(&tune);
    let mut sim_state = sim_state;
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(712.0, 120.0); // outboard in x, well ABOVE the lip (node 1 y=200)
    fighter.vel = Vector2::new(0.0, 400.0); // already falling past ledge_fall_eps
    let after_one = step(&sim_state, &[&IDLE, &IDLE], &tune);
    assert_ne!(
        after_one.fighters[0].state,
        CharState::LedgeHold,
        "must not grab on the very first frame while still 72px above the lip"
    );
    let mut current = after_one;
    let mut ever_grabbed = false;
    for _ in 0..30 {
        current = step(&current, &[&IDLE, &IDLE], &tune);
        if current.fighters[0].state == CharState::LedgeHold {
            ever_grabbed = true;
            break;
        }
    }
    assert!(
        ever_grabbed,
        "sanity: falling outboard past the lip still eventually grabs it"
    );
}

#[test]
fn short_ink_segment_emits_no_lip_long_one_does() {
    // plans/ledge-ship-fixes.md #2: a Ledge segment shorter than `ledge_min_len` is a scribble, not
    // a real edge -- no lip, either endpoint. A long one still emits.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut sim_state = SimState::spawn();
    sim_state.active = 1;
    let half = tune.ledge_min_len * 0.5;
    let short_top = 560.0 + half;
    let short = [
        Vector2::new(560.0, 200.0),
        Vector2::new(short_top, 200.0),
        Vector2::new(short_top, 320.0),
    ];
    sim_state.paths[0] = rehydrate_stroke(
        &short,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    );
    let lips = crate::v1::body::LipSoup::collect(&sim_state.paths, &sim_state.nodes, &tune);
    assert!(
        !lips.lips().iter().any(|l| l.owner == 0),
        "a Ledge segment under ledge_min_len must not emit a lip"
    );

    let double = tune.ledge_min_len * 2.0;
    let long_top = 560.0 + double;
    let long = [
        Vector2::new(560.0, 200.0),
        Vector2::new(long_top, 200.0),
        Vector2::new(long_top, 320.0),
    ];
    sim_state.paths[0] = rehydrate_stroke(
        &long,
        0,
        0,
        &mut sim_state.nodes,
        &mut sim_state.free,
        &tune,
    );
    let lips = crate::v1::body::LipSoup::collect(&sim_state.paths, &sim_state.nodes, &tune);
    assert!(
        lips.lips().iter().any(|l| l.owner == 0),
        "a Ledge segment over ledge_min_len must emit a lip"
    );
}

// ── plans/ledge-ship-fixes.md #4: down-edge-after-fastfall opt-out ──────────────────────────────────

#[test]
fn held_down_through_apex_still_grabs_the_ledge() {
    // Holding down through the apex sets `fast_falling` (body/mod.rs) but produces NO
    // `down_pressed` edge afterward -- ledges must still grab (held-down alone is not an opt-out).
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let (mut sim_state, _slot) = l_stroke_scene(&tune);
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    fighter.pos = Vector2::new(712.0, 235.0);
    fighter.vel = Vector2::new(0.0, 400.0);
    fighter.fast_falling = true; // already fast-falling, as if held down through the apex
    let mut current = sim_state;
    let mut caught = false;
    for _ in 0..10 {
        current = step(&current, &[&IDLE, &IDLE], &tune); // IDLE never fires down_pressed
        if current.fighters[0].state == CharState::LedgeHold {
            caught = true;
            break;
        }
    }
    assert!(
        caught,
        "holding down through the apex must not opt out of the ledge grab"
    );
}

#[test]
fn repress_down_after_fastfall_skips_the_grab() {
    // A release + fresh press of down WHILE ALREADY fast-falling is the opt-out edge: the fresh
    // `down_pressed` this frame, combined with `fast_falling` already true, skips the snap attempt
    // -- and the fast-fall speed itself carries the body straight through the rest of the catch
    // band before the next frame (no fresh edge) gets another chance.
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let (mut sim_state, _slot) = l_stroke_scene(&tune);
    let fighter = &mut sim_state.fighters[0];
    fighter.state = CharState::Air;
    fighter.ground_plat = -1;
    // deep in the catch band already (close to the far edge of `ledge_reach_down`), fast-falling:
    // skipping just this one frame's attempt is enough for the fastfall speed to carry it out past
    // the band before the next unskipped frame runs.
    fighter.pos = Vector2::new(712.0, 200.0 + tune.ledge_reach_down - 5.0);
    fighter.vel = Vector2::new(0.0, tune.fastfall);
    fighter.fast_falling = true;
    let repress = InputFrame {
        down: true,
        down_pressed: true,
        aim_y: 1.0,
        ..IDLE
    };
    let mut current = step(&sim_state, &[&repress, &IDLE], &tune);
    assert_ne!(
        current.fighters[0].state,
        CharState::LedgeHold,
        "the repress frame itself must not catch"
    );
    for _ in 0..10 {
        current = step(&current, &[&IDLE, &IDLE], &tune);
        assert_ne!(
            current.fighters[0].state,
            CharState::LedgeHold,
            "opted out for this pass, must sail past rather than catch a frame later"
        );
    }
}
