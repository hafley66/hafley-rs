// Playtest bug (2026-07-06): "holding a direction (left/right) into ink, the fighter slips
// straight through purple ink strokes and through the ship hull -- nothing kept me in the ship
// hull while I held left into and out of it." Happens at WALK speed (slow, sustained press),
// not just fast diagonals; affects any/all purple (Wall-classified) player strokes AND the
// baked hull rim.
//
// Root cause: `sweep_walls` (body/mod.rs) samples a Wall segment's own bounded y-span at ONE
// height per frame -- the ECB center, `cy = pos.y - ECB_HALF_H` (70px above the feet). A
// GROUNDED WALK (not a fall) holds that sampled height essentially constant while it approaches
// a Floor-to-Wall JOINT: if the Wall segment right at that joint is shorter than ECB_HALF_H --
// an entirely plausible shape for a hand-drawn zigzag stroke (`min_seg` only floors segments at
// 10px) or a discretized rim near a bend -- the sampled height never lands inside that short
// wall's own span, on ANY frame, however slowly the fighter approaches: a speed-INDEPENDENT gap
// at the segment join, distinct from the already-fixed fast-diagonal dome-tunneling bug
// (`sweep_solid_ink_crossing`, which is itself gated OFF below a body-width's worth of
// displacement and so never engages at walk speed regardless).
//
// Declared as a crate-root child (`use super::*`) so `super::*` is the crate root, same
// convention as every other `*_tests.rs`.

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

fn tune() -> Tune {
    Tune::from_char(&CharData::KNEEMAN)
}

#[test]
fn walking_at_walk_speed_into_a_short_wall_joint_stays_blocked() {
    let t = tune();
    let mut s = SimState::spawn();
    // A plain default-pen stroke (row 0, `solid: false` -- an ORDINARY purple wall, the material
    // every player's freehand wall draws unless they force-solid it): a flat floor running into
    // a SHORT (40px, under ECB_HALF_H's 70px) near-vertical wall segment, planted in clear
    // airspace well away from every other spawn fixture.
    const SLOT: usize = 5;
    let stroke = rehydrate_stroke(
        &[
            Vector2::new(600.0, 200.0),
            Vector2::new(680.0, 200.0),
            Vector2::new(680.0, 160.0),
        ],
        0,
        0,
        &mut s.nodes,
        &mut s.free,
        &t,
    );
    assert!(
        matches!(
            stroke.seg_class(0, &s.nodes),
            SegClass::Floor | SegClass::Ledge
        ),
        "sanity: the approach segment is walkable (Floor/Ledge, may be a lip at the open end): {:?}",
        stroke.seg_class(0, &s.nodes)
    );
    assert_eq!(
        stroke.seg_class(1, &s.nodes),
        SegClass::Wall,
        "sanity: the joint segment classifies as a Wall"
    );
    s.paths[SLOT] = stroke;
    s.fighters[0].pos = Vector2::new(600.0, 200.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].ground_ink = SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    // WALK_THRESH (0.25) < 0.35 < DASH_THRESH (0.5): a sustained analog press, walk speed only.
    let hold_right = InputFrame { dir: 0.35, ..IDLE };
    for frame in 0..180 {
        s = step(&s, &[&hold_right, &IDLE], &t);
        let f = &s.fighters[0];
        assert!(
            f.pos.x <= 680.0 + 5.0,
            "the wall joint must stay blocked while WALKING into it (frame {frame}): pos={:?}",
            f.pos
        );
    }
}

// Playtest bug (2026-07-07): "a strong hit (an ink strike) launched me INTO the main stage and I
// fell straight through the bottom to my death" -- the main-stage counterpart of the ink-rim
// tunneling fixed in 83085cd, on the stage's own two side walls (`Soup::collect`'s `FLOOR_LEFT`/
// `FLOOR_RIGHT` faces), which still ride the old point-sample `sweep_walls` (ink is routed through
// `sweep_ink_containment`'s box-vs-segment SAT resolver instead -- "INK ONLY", body/mod.rs).
//
// Root cause: `sweep_walls` sampled a Wall segment's own bounded y-span at ONE height per frame,
// the ECB CENTER (`cy = pos.y - ECB_HALF_H`), not the ECB's actual feet-to-head extent. The main
// stage's side faces span only `y in [GROUND_Y, STAGE_BOTTOM]` (760..900, a 140px-tall wall, same
// height as the ECB itself). A fighter whose FEET sit just below the lip -- `feet_y` in
// `(GROUND_Y, GROUND_Y + ECB_HALF_H)` -- has `cy` ABOVE `GROUND_Y`, outside the wall's own span, so
// the single-height sample missed it outright even though the box's lower half plainly overlaps
// the wall. `combat.rs` sets the full launch velocity in one shot (`self.vel = l.vel`); the very
// next tick's one large step lands the body past the wall's face with no floor to catch it either
// (`sweep_floors` only catches a crossing-from-above landing, and the body is already below the
// lip height) -- "falls through the middle of the stage."
fn strong_launch_into_a_stage_wall_below_the_lip_stays_blocked(sgn: f32) {
    let t = tune();
    let mut s = SimState::spawn();
    // Clear the parked hull (skill's "mind the spawn area" trap): its right edge sits only 34px
    // shy of `FLOOR_LEFT` (`ship_home`'s doc, stage/mod.rs), and its lower dome shoulder facet
    // runs close enough overhead that the LEFT variant's full-height ECB (140px, extending up to
    // `feet_y - 2*ECB_HALF_H` once this fn's own fix widens `sweep_walls`' extent test) grazes it
    // independently -- a real, already-solved, out-of-scope `sweep_ink_containment` concern (INK
    // ONLY, body/mod.rs), not a `sweep_walls` defect. This repro is about the stage's own side
    // faces in isolation, so the hull is not part of the fixture.
    s.paths[SHIP_SLOT] = InkPath::EMPTY;
    // feet 30px below the lip: cy = feet_y - ECB_HALF_H sits ABOVE GROUND_Y, outside the wall's
    // own [GROUND_Y, STAGE_BOTTOM] span by the old single-height sample -- exactly the missed band.
    let feet_y = GROUND_Y + 30.0;
    let wall_x = if sgn > 0.0 { FLOOR_RIGHT } else { FLOOR_LEFT };
    let start_x = wall_x + sgn * 60.0; // just outside the face
    s.fighters[0].pos = Vector2::new(start_x, feet_y);
    // large inward horizontal launch (order 4000+ px/s): one DT step (~1/60s) crosses ~75px,
    // well past the wall's face and deep into the stage interior if it isn't caught.
    s.fighters[0].vel = Vector2::new(-sgn * 4500.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    // parked in clear airspace (skill's spawn-area map), well off the launch fighter's path.
    s.fighters[1].pos = Vector2::new(600.0, 100.0);
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    s.fighters[1].ground_ink = -1;
    s = step(&s, &[&IDLE, &IDLE], &t);
    let f = &s.fighters[0];
    let outside = if sgn > 0.0 {
        f.pos.x >= FLOOR_RIGHT + ECB_HALF_W - 1.0
    } else {
        f.pos.x <= FLOOR_LEFT - ECB_HALF_W + 1.0
    };
    assert!(
        outside,
        "one frame of a strong launch into the stage wall's below-the-lip band must stop the \
         fighter flush outside the wall, not let it tunnel into the stage interior: pos={:?}",
        f.pos
    );
}

#[test]
fn strong_launch_into_the_right_stage_wall_below_the_lip_stays_blocked() {
    strong_launch_into_a_stage_wall_below_the_lip_stays_blocked(1.0);
}

#[test]
fn strong_launch_into_the_left_stage_wall_below_the_lip_stays_blocked() {
    strong_launch_into_a_stage_wall_below_the_lip_stays_blocked(-1.0);
}
