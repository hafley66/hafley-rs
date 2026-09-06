// Ship containment + 2026-07-04 playtest follow-ups, split from ship_tests.rs (file budget):
// the dome lateral-block/containment arc (the Part A/B tunneling bug and its container rework),
// strike-from-inside, the no-rotation row, and the zone-carrying hull. Declared as a crate-root
// child so `super::*` is the crate root.

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

/// Fighter 0 dropped onto the dome and settled inside the bowl; fighter 1 parked on the main
/// stage (same fixture as ship_tests::crewed).
fn crewed() -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R - 40.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    (s, t)
}

// ── Part A/B playtest bug + fix (plans/body-unify.md step 5 follow-up) ────────────────────────────
//
// Report: "was able to hit other char into ship but blue never stopped them and they fell thru."
// Root cause: the hull's mid-slope "shoulder" segments classify as `SegClass::Floor`
// (StrokeProps::PEN: "Blue (Floor/Ledge) segments are SOFT"), and `Surf`'s Floor kind had NO
// horizontal collision anywhere in the movement lane -- `sweep_floors` only ever catches a body
// crossing a Floor's height going DOWN (a landing), never one arriving from the side. Only the
// near-vertical band (`SegClass::Wall`) blocked sideways, via `sweep_walls` (the control test
// below pins that it's fine, isolating the bug to the Floor-classified shoulder).
//
// Fixed by `body::sweep_gated_floor_lateral` (Part A: a swept-capsule lateral crossing test) +
// the hull's material row (Part B). Part B has since moved on: the step-5 `PassBackward`
// exit-only rim let crew "randomly clip out of blue things on the weird sloping parts"
// (2026-07-04 playtest), so the hull is now a plain SOLID container -- both directions block,
// the hatch is the one door -- and the lateral sweep covers Solid ink-owned Floor rows too.

#[test]
fn hitstun_launch_into_dome_equator_wall_blocks() {
    // Control: same launch shape as the failing shoulder test below, aimed at the EQUATOR instead
    // (y=520, segments 3/4, classified Wall). This must block -- pins that `sweep_walls` runs and
    // works correctly in `hitstun_slide`, isolating the bug to the Floor-classified shoulder band.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x + 400.0, SHIP_HOME.y);
    s.fighters[0].vel = Vector2::new(-600.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    let f = &s.fighters[0];
    assert!(
        (f.pos - SHIP_HOME).length() >= SHIP_R - 20.0,
        "the equator wall must block: final pos={:?} dist_from_center={}",
        f.pos,
        (f.pos - SHIP_HOME).length()
    );
}

#[test]
fn hitstun_launch_into_dome_shoulder_blocks() {
    // Part A fix (sweep_gated_floor_lateral, body/mod.rs) + Part B (the hull's Floor rows are now
    // GateSide::PassBackward): the shoulder band -- segments 1-2, Floor-classified, moderately
    // sloped, span roughly x in [-10, 101], y in [272, 425] world (SHIP_HOME.x=-190) -- now blocks
    // an outside-arriving body laterally instead of tunneling through. At y=350 the hull boundary
    // sits at about x=62.6. Launch a tumbling fighter from OUTSIDE that boundary (x=210, well clear
    // of it) moving left at a moderate smash-attack speed, so it approaches the shoulder from the
    // SIDE rather than falling onto it from above.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x + 400.0, SHIP_HOME.y - 170.0);
    s.fighters[0].vel = Vector2::new(-600.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    let f = &s.fighters[0];
    assert!(
        (f.pos - SHIP_HOME).length() >= SHIP_R - 20.0,
        "the shoulder must now BLOCK a body arriving from outside, same as the equator control: \
         final pos={:?} dist_from_center={}",
        f.pos,
        (f.pos - SHIP_HOME).length()
    );
}

#[test]
fn inside_launch_blocked_by_the_blue_shoulder_and_the_solid_equator_bounces() {
    // RETIRED 2026-07-07 (playtest: "blue = u can pass when u come towards it"): the hull's Floors
    // are SOFT now (drop-through platforms), and Solid Floors no longer block lateral crossings
    // (sweep_ink_containment's kind-aware fence skips every non-OneWay Floor). The "solid bouncy
    // container, exit only via the hatch" model this test pinned is inverted -- the spec is now
    // "blue = pass through", walls (purple) are the sole lateral blocker. The test was deleted;
    // this stub keeps the symbol out of the way if a grep references the old name. Re-asserting
    // container containment here would re-pin the behavior the user explicitly rejected.
    let _ = (SimState::spawn, tune(), SHIP_HOME, SHIP_R, GROUND_Y);
}

// ── 2026-07-04 playtest follow-ups ────────────────────────────────────────────────────────────────

#[test]
fn strike_from_inside_the_bowl_moves_the_hull() {
    // "hitting ship from inside moves it, no": there is NO side filter in the strike lane --
    // `strike_ink` hits any segment within the hitbox circle, whichever side it's approached
    // from. What the playtest actually saw is REACH: from the cockpit the only surface in range
    // is the bowl floor at your feet, and chest-height boxes (jab/ftilt) whiff it, exactly as
    // they whiff the main stage's floor. This pins the lane itself: a contact that DOES reach
    // the interior floor launches the hull the same as an outside hit.
    let t = tune();
    let mut s = SimState::spawn();
    let bowl_floor = ink_floor_y_at(&s.paths[SHIP_SLOT], SHIP_HOME.x, &s.nodes)
        .expect("bowl floor spans dead center");
    let contact_probe = Vector2::new(SHIP_HOME.x, bowl_floor - 10.0); // just above the floor, inside
    let hb = t.fsmash.boxes[0];
    let hit = stage::strike_ink(
        &mut s.paths,
        contact_probe,
        60.0,
        &hb,
        hb.damage,
        1.0,
        &s.nodes,
        &t,
    );
    assert!(
        hit,
        "a strike reaching the interior floor connects from inside"
    );
    assert!(
        s.paths[SHIP_SLOT].percent > 0.0,
        "the hull took the damage: percent={}",
        s.paths[SHIP_SLOT].percent
    );
    assert!(
        s.paths[SHIP_SLOT].vel.length() > 0.0 || s.paths[SHIP_SLOT].shake > 0,
        "the hull reacts (launch or shake): vel={:?} shake={}",
        s.paths[SHIP_SLOT].vel,
        s.paths[SHIP_SLOT].shake
    );
}

#[test]
fn struck_hull_never_rotates() {
    // "why is ship rotating its inks": off-center strikes/grazes wrote `omega`, and the tumble
    // turned the hatch and station anchors with it. The hull's `spin_scale` row is 0.0 --
    // whatever writes `omega`, `rot` never accrues (integrate_ink zeroes it before the accrual).
    let t = tune();
    let mut s = SimState::spawn();
    // an off-center contact on the right equator, aimed to torque: force a launch + spin write
    let equator = SHIP_HOME + Vector2::new(SHIP_R, 0.0);
    let hb = t.fsmash.boxes[0];
    stage::strike_ink(
        &mut s.paths,
        equator,
        60.0,
        &hb,
        hb.damage * 3.0,
        1.0,
        &s.nodes,
        &t,
    );
    for _ in 0..30 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_eq!(
            s.paths[SHIP_SLOT].rot, 0.0,
            "the hull never turns, whatever spin the strike wrote (omega={})",
            s.paths[SHIP_SLOT].omega
        );
    }
}

#[test]
fn flying_hull_bounces_off_the_blast_walls() {
    // The blast frame is HARD WALLS for the ship (2026-07-04 director's call, replacing the
    // same-day zone-carrying experiment): a hull flying at the left blast line reflects off it
    // (with its own bounce row) instead of leaving the arena, so the ship and its crew never
    // cross a kill line. The fighters' static KO rect is unchanged (`zone_mode: Static`).
    let t = tune();
    let mut s = SimState::spawn();
    // park the hull just inside the left blast line, flying hard left
    s.paths[SHIP_SLOT].pos = Vector2::new(BLAST_LEFT + SHIP_R + 40.0, SHIP_HOME.y);
    s.paths[SHIP_SLOT].vel = Vector2::new(-8.0, 0.0); // native px/frame
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    let mut bounced = false;
    for _ in 0..120 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        let (center, radius) = s.paths[SHIP_SLOT].bound_circle(&s.nodes);
        assert!(
            center.x - radius >= BLAST_LEFT - 1.0,
            "the hull never crosses the blast wall: left edge at {}",
            center.x - radius
        );
        if s.paths[SHIP_SLOT].vel.x > 0.0 {
            bounced = true;
            break;
        }
    }
    assert!(bounced, "the wall reflected the hull's velocity");
}

// ── 2026-07-05 playtest: fast-diagonal dome tunneling ────────────────────────────────────────────
//
// Report: INSIDE the parked hull, moving at full air speed on a steep down-right diagonal (max
// horizontal drift + committed fastfall, jump held), the fighter passes straight through the rim
// arc. Root cause is per-axis sweeps missing on a fast diagonal: `sweep_walls` samples the wall
// span only at THIS frame's ECB center height, and a fast descent crosses the equator band's whole
// y-span in the same frame it crosses in x; the sloped bowl-floor rim has no lateral block at all
// (`sweep_floors` only catches a downward LANDING, `sweep_gated_floor_lateral` only its FLOOR-kind
// rows and only under body-width per-frame steps). The whole-displacement swept test
// `body::sweep_solid_ink_crossing` catches the crossing both per-axis sweeps miss.

fn assert_diagonal_contained(sgn: f32) {
    let t = tune();
    let mut s = SimState::spawn();
    // Start at dead center, heading for the lower BLUE bowl floor on a steep fastfall diagonal
    // (horizontal damped to 0.35 so the crossing lands on the Floor-classified bowl bottom, not
    // the one-way purple equator -- a 45-degree diagonal now legitimately EXITS the equator, tested
    // in inside_launch_blocked_by_the_blue_shoulder_then_exits_the_one_way_equator + hull_gate_tests).
    // Still fast on both axes (past the ECB half-width of 38), so the swept solid-ink crossing
    // backstop (the 2954822 fast-diagonal fix) is exercised on the SOLID blue rim.
    s.fighters[0].pos = SHIP_HOME;
    s.fighters[0].vel = Vector2::new(sgn * t.fastfall * 0.35, t.fastfall);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].fast_falling = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    // no lateral drift input: a sustained `dir: sgn` push would carry the body to the one-way
    // equator and out (the intended exit); this test keeps it aimed at the blue bowl bottom.
    let hold = InputFrame {
        jump_held: true,
        ..IDLE
    };
    let start = (s.fighters[0].pos - SHIP_HOME).length();
    assert!(
        start < SHIP_R - 20.0,
        "sanity: must start inside, dist={start}"
    );
    for frame in 0..40 {
        s = step(&s, &[&hold, &IDLE], &t);
        let dist = (s.fighters[0].pos - SHIP_HOME).length();
        assert!(
            dist < SHIP_R + 20.0,
            "the solid rim must contain a fastfall diagonal (sgn={sgn} frame={frame} dist={dist} pos={:?})",
            s.fighters[0].pos
        );
    }
}

#[test]
fn fastfall_diagonal_inside_hull_stays_contained() {
    assert_diagonal_contained(1.0); // down-right
}

#[test]
fn fastfall_diagonal_inside_hull_stays_contained_mirrored() {
    assert_diagonal_contained(-1.0); // down-left
}

// ── row 5: hull-vs-stage-ink layering (plans/ship-containment.md §4) ─────────────────────────────
//
// Root: `Soup::collect` (body/mod.rs) concatenates the hull's own rim ink with every OTHER
// drawn/baked ink path into one flat list, and `sweep_walls` returns the FIRST BLOCKING candidate
// it finds scanning that list in index order -- no owner precedence at all. A foreign ink wall
// (an ordinary player-drawn stroke, unrelated to the hull) that happens to sit INSIDE the hull's
// bounding volume shadows the container's own boundary: a body genuinely contained by the hull
// snaps to wherever that foreign stroke sits, not to the hull's true rim.

#[test]
fn contained_body_passes_a_foreign_ink_wall_and_still_meets_the_true_hull_rim() {
    let t = tune();
    let mut s = SimState::spawn();
    // A foreign, non-hull SOLID wall (owner 0, an ordinary settled player stroke -- nothing to do
    // with the hull's own SHIP_SLOT path) sitting INSIDE the hull's bounding circle, well short of
    // the hull's own equator-height rim segment (the control launch below this one crosses that
    // real boundary at world x ~= 108, a hair short of the naive `SHIP_HOME.x + SHIP_R` = 116 --
    // the rim is a discretized polygon, not a perfect circle). The band is tall (+-250) because
    // `sweep_walls` samples the ECB CENTER height (`pos.y - ECB_HALF_H`, 70px above the feet), not
    // the feet themselves.
    let foreign_x = SHIP_HOME.x + 230.0;
    let foreign = rehydrate_stroke(
        &[
            Vector2::new(foreign_x, SHIP_HOME.y - 250.0),
            Vector2::new(foreign_x, SHIP_HOME.y + 250.0),
        ],
        0,
        0,
        &mut s.nodes,
        &mut s.free,
        &t,
    );
    assert_eq!(
        foreign.seg_class(0, &s.nodes),
        SegClass::Wall,
        "sanity: the foreign stroke classifies as a Wall"
    );
    s.paths[0] = foreign;
    // Fighter inside the hull, a SHORT gap from the foreign wall -- crossed in a couple of frames,
    // long before hitstun's gravity curves the arc away from the equator line (a longer runway
    // dumps the body toward the bowl bottom instead, a different bug). `ground_ink = -1` matches
    // every other dome-containment test above: a body mid-launch INSIDE the hull is not "on" any
    // surface this frame, but it is still geometrically contained.
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x + 206.0, SHIP_HOME.y);
    s.fighters[0].vel = Vector2::new(600.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    // Track the PEAK x reached, not just the final resting spot: hitstun's gravity curves the
    // trajectory down off the equator line as the frames go on (same as every other hull test in
    // this file), so "final dist from center" drifts for reasons unrelated to this bug. The
    // discriminator is simpler and robust to that curvature: did it ever get meaningfully PAST the
    // foreign wall's x, or did the wrong surf cap its reach right there.
    let mut peak_x = f32::MIN;
    for _ in 0..15 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        let f = &s.fighters[0];
        peak_x = peak_x.max(f.pos.x);
        let dist = (f.pos - SHIP_HOME).length();
        assert!(
            dist < SHIP_R + 20.0,
            "the solid hull must still contain it (the true rim, once actually reached): \
             dist={dist} pos={:?}",
            f.pos
        );
    }
    assert!(
        peak_x > foreign_x + 10.0,
        "a contained body must sail past a foreign ink wall inside the hull, reaching well beyond \
         its x={foreign_x} before the hull's OWN rim ever gets a say: peak x reached={peak_x} \
         (capped at/near the foreign wall means the wrong surf caught it, no owner precedence)",
    );
}

// ── 2026-07-06 playtest: walk-speed joint tunneling ───────────────────────────────────────────────
//
// Report: "holding a direction (left/right) into ink, the fighter slips straight through purple
// ink strokes and through the ship hull... nothing kept me in the ship hull while I held left
// into and out of it." Happens at WALK speed, not the fast diagonal the row above already fixed.
// Root cause: `sweep_walls` samples a Wall segment's own bounded y-span at ONE height per frame
// (the ECB center); a GROUNDED WALK holds that sampled height essentially constant while crossing
// a Floor-to-Wall JOINT on the hull's own discretized rim, and can miss it regardless of speed.
// Fixed by `body::solid_ink::sweep_wall_joints`, an ungated (any-speed) capsule sweep over every
// Wall-kind, fully-blocking (`Solid`/`Soft`) segment -- the endpoint discs cover the joint the
// single-height sample misses.

#[test]
fn walking_at_walk_speed_inside_the_bowl_stays_contained() {
    let (mut s, t) = crewed(); // settled on the bowl floor, same fixture as the tests above
    // WALK_THRESH (0.25) < 0.35 < DASH_THRESH (0.5): a sustained analog press, walk speed only --
    // no dash, no aerial drift, the exact "held a direction" shape from the report.
    let hold_left = InputFrame { dir: -0.35, ..IDLE };
    for frame in 0..300 {
        s = step(&s, &[&hold_left, &IDLE], &t);
        let f = &s.fighters[0];
        let dist = (f.pos - SHIP_HOME).length();
        assert!(
            dist < SHIP_R + 20.0,
            "walking into the hull's own rim at WALK speed must stay contained (frame {frame}): \
             dist={dist} pos={:?}",
            f.pos
        );
    }
}

#[test]
fn slow_rise_from_inside_never_leaks_through_the_overhead_rim() {
    // 2026-07-06 playtest, the SECOND report (plans/ship-containment.md): "still slipping through
    // the purple inner line at the top of the hull, from inside, on a slow rise near apex." The
    // original Part A/B fix only caught a WHOLE-DISPLACEMENT sweep past a body-width, or a body
    // riding `ground_ink` that just released it -- an airborne body climbing slowly toward the
    // hull's own near-top rim has near-zero per-frame displacement (the retired circle sweep's
    // size gate never fired) and never rides `ground_ink` at all (the retired joint sweep's FSM
    // gate never fired either): nothing caught it. `sweep_ink_containment` is unconditional on
    // both, so this must hold now.
    //
    // Segment 0 of the baked hull (x in [-95.44, -10.14], y in [229, 272], `SegClass::Ledge` --
    // one of the hatch-flank lips, but `Randall for InkPath`'s `SegClass::Floor | SegClass::Ledge`
    // arm emits a Floor-kind Ink Surf for it same as any shoulder segment) is the real hatch-
    // adjacent rim, NOT the hatch gap itself (the open hole spans roughly x in [-284.56, -95.44]
    // at this height) -- x=-70 sits mid-span, clear of both the gap and the next segment's own
    // junction.
    let t = tune();
    let mut s = SimState::spawn();
    // y=340 starts comfortably inside the hull's own bounding circle (dist ~162 from SHIP_HOME,
    // well under SHIP_R=306) but already within this segment's own ECB-box SAT reach (its normal
    // is diagonal, so the box's relevant reach along it is bigger than the segment's raw pixel
    // height alone) -- exactly the near-apex approach the report describes, not a distant flight
    // in. Gravity alone would arc a single upward kick back down long before reaching the rim, so
    // each frame re-asserts a small, steady rise (`vel.y=-120`, 2px/frame) -- a sustained slow
    // float, not a one-off launch, isolating the resolver from unrelated fall-arc physics. The
    // resolver catches and rests it well before the rim (see the loop's own assert bound), then
    // the sustained rise re-approaches and re-catches -- repeatedly contained, never through.
    s.fighters[0].pos = Vector2::new(-70.0, 340.0);
    s.fighters[0].vel = Vector2::new(0.0, -120.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    for frame in 0..120 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        s.fighters[0].vel.y = -120.0; // hold the slow, steady rise against gravity's pull
        let f = &s.fighters[0];
        assert!(
            (f.pos - SHIP_HOME).length() < SHIP_R + 20.0,
            "a slow rise from inside must never leak through the overhead rim (frame {frame}): \
             pos={:?} dist={}",
            f.pos,
            (f.pos - SHIP_HOME).length()
        );
    }
}
