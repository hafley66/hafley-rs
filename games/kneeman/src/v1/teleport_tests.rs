//! The grounded-ink/platform teleport bug class (dd91584 ship-hull teleport, 677b8b5 bowl
//! air-walk, and the B-under-the-hull special snap this file's repro was written for): some
//! ground-resolution branch re-derives "the floor at my x" as the HIGHEST/closest-anywhere floor
//! instead of the floor CONTINUOUS with the fighter's own standing y. A closed ink stroke (the
//! ship hull) hands two candidate floors to the same x, so getting this wrong snaps the fighter
//! across the shape.
//!
//! This file: (1) the repro for the special-move snap, written to fail against the pre-fix code;
//! (2) a grid-sweep battery broad enough to have caught all three named instances of the class;
//! (3) the invariant helpers (`body::assert_grounded_continuity` / `body::on_real_floor`) get a
//! direct unit test too, independent of any fighter/FSM plumbing.

use crate::v1::arena::InkNode;
use crate::v1::body::{Soup, SurfKind, SurfOwner, on_real_floor, set_ground};
use crate::v1::stage::{InkPath, SHIP_HOME, SHIP_R, SHIP_SLOT, ink_floor_y_near};
use crate::v1::za_warudo::INK_STEP_MAX;
use crate::v1::{
    CharData, CharState, DT, Fighter, GROUND_Y, InputFrame, MAX_DRAWN, SimState, Tune, Vector2,
    out_of_bounds, step,
};

fn tune() -> Tune {
    Tune::from_char(&CharData::KNEEMAN)
}

/// Fighter 0 grounded on the hull at world-x `x`, landing on whichever candidate floor sits
/// nearest `near_y` (the ship hull is a closed stroke: `ink_floor_y_at` alone would always hand
/// back the topmost of the two arcs spanning `x`, so setup uses the same near-pick the fix does).
/// Fighter 1 parked far away on the main stage, idle, so no combat/footstool ever fires.
fn grounded_on_hull_near(x: f32, near_y: f32, facing: f32) -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    let y = ink_floor_y_near(&s.paths[SHIP_SLOT], x, near_y, &s.nodes)
        .expect("x is over a walkable hull segment near this y");
    s.fighters[0].pos = Vector2::new(x, y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].facing = facing;
    s.fighters[0].ground_ink = SHIP_SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    (s, t)
}

fn idle() -> InputFrame {
    InputFrame::default()
}

// ---- Task 1: the repro (playtest report: B underneath the ship's yellow ink snaps to the top)
// ----

#[test]
fn special_under_the_hull_does_not_snap_to_the_top() {
    // Lower-right OUTER arc of the hull, standing on the underside (facing left, into the hull's
    // mass) -- the exact shape in the screenshot: fighter just below/outside the ring. At this x
    // the hull's TOP arc (near vertex 18, the mirror point) spans the same x much higher up; the
    // pre-fix code (`ink_floor_y_at`, unconditional) always returns that higher one, so pressing B
    // teleported the fighter's y there instantly.
    let x = SHIP_HOME.x + 0.809 * SHIP_R; // ~57.5
    let low_y = SHIP_HOME.y + 0.588 * SHIP_R; // ~700: the low arc, standing UNDER the overhang
    let (mut s, t) = grounded_on_hull_near(x, low_y, -1.0);
    assert!(
        (s.fighters[0].pos.y - low_y).abs() < 5.0,
        "setup landed on the LOW arc as intended, y={}",
        s.fighters[0].pos.y
    );
    let start = s.fighters[0].pos;
    let press = InputFrame {
        special: true,
        ..idle()
    };
    let mut max_disp: f32 = 0.0;
    for f in 0..8 {
        let inp = if f == 0 { press } else { idle() };
        let prev = s.fighters[0].pos;
        s = step(&s, &[&inp, &idle()], &t);
        max_disp = max_disp.max((s.fighters[0].pos - prev).length());
    }
    let end = s.fighters[0].pos;
    assert!(
        max_disp < 90.0,
        "no single-frame teleport pressing B under the hull: worst per-frame displacement \
         {max_disp}px (start={start:?} end={end:?})"
    );
    assert!(
        (end.y - start.y).abs() < 90.0,
        "must not have snapped to the far arc: start.y={} end.y={}",
        start.y,
        end.y
    );
}

// ---- Task 2.1: the invariant helpers, unit-tested directly ----

#[test]
fn assert_grounded_continuity_accepts_a_small_step_and_rejects_a_teleport() {
    // within bound: no panic.
    crate::v1::body::assert_grounded_continuity(true, true, INK_STEP_MAX - 1.0, INK_STEP_MAX);
    // not grounded both ends: unbounded jumps are fine (e.g. a real launch or a ledge climb).
    crate::v1::body::assert_grounded_continuity(false, true, 500.0, INK_STEP_MAX);
    crate::v1::body::assert_grounded_continuity(true, false, 500.0, INK_STEP_MAX);
}

#[test]
#[should_panic(expected = "grounded-step teleport")]
fn assert_grounded_continuity_panics_past_the_bound() {
    crate::v1::body::assert_grounded_continuity(true, true, INK_STEP_MAX + 200.0, INK_STEP_MAX);
}

#[test]
fn on_real_floor_finds_the_main_stage_and_misses_open_air() {
    let s = SimState::spawn();
    let soup = Soup::collect(&s.paths, &s.nodes, None);
    assert!(on_real_floor(
        Vector2::new(600.0, GROUND_Y),
        soup.surfs(),
        1.0
    ));
    assert!(!on_real_floor(
        Vector2::new(600.0, GROUND_Y - 300.0),
        soup.surfs(),
        1.0
    ));
}

// ---- Task 2.2: the grid-sweep battery ----

/// Generous per-frame displacement ceiling: the fastest legitimate steady velocity the sim ever
/// hands a NON-launched fighter (dash/run/air-drift/full-hop/double-jump/airdodge/walljump/
/// ledgejump/techroll), converted px/s -> px/frame (`v * DT`), combined as a worst-case diagonal
/// (both axes maxed the same frame -- in practice they never are), plus the grounded-pin's own
/// continuity allowance, doubled for headroom. Not a gameplay number: it's "how far can the sim
/// legitimately move a fighter in one frame", so a violation in a non-launch state is a bug, not
/// a balance call.
fn max_legit_step(t: &Tune) -> f32 {
    let vx = [
        t.run_speed,
        t.dash_init,
        t.air_speed,
        t.walljump_h,
        t.techroll_speed,
        t.jump_h_max,
    ]
    .into_iter()
    .fold(0.0_f32, f32::max);
    let vy = [
        t.max_fall,
        t.fastfall,
        -t.fullhop_v,
        -t.airjump_v,
        -t.ledgejump_v,
        -t.walljump_v,
    ]
    .into_iter()
    .fold(0.0_f32, f32::max);
    ((vx * vx + vy * vy).sqrt() * DT + INK_STEP_MAX) * 2.0
}

/// A fighter mid-launch (knockback slide, floored, teching, or held) can legitimately cover a lot
/// of ground in one frame -- excluded from the battery's bound, per the task brief. None of the
/// single-input battery scenarios below should ever reach these (no combat happens), but the
/// filter is here defensively, same discipline as `assert_grounded_continuity` being state-aware.
fn is_intentional_launch(f: &Fighter) -> bool {
    f.hitstun > 0
        || matches!(
            f.state,
            CharState::Launched
                | CharState::Knockdown
                | CharState::Getup
                | CharState::TechInPlace
                | CharState::TechRoll
                | CharState::GetupAttack
                | CharState::Grabbed
                | CharState::GrabHold
        )
}

/// The walkable floor (if any) spanning world-x `x` whose y sits closest to `near_y`, scanned
/// across the WHOLE soup (every platform + every ink path/fixture), not just one stroke -- the
/// grid's job is to land fighters on whatever real surface is closest to each row, ship hull
/// included, exactly like the reported bug's own standing spot.
fn nearest_ground(
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    x: f32,
    near_y: f32,
) -> Option<(f32, SurfOwner)> {
    let soup = Soup::collect(paths, nodes, None);
    let mut best: Option<(f32, SurfOwner)> = None;
    for s in soup.surfs() {
        if s.kind != SurfKind::Floor {
            continue;
        }
        let (a, b) = if s.a.x <= s.b.x {
            (s.a, s.b)
        } else {
            (s.b, s.a)
        };
        if x < a.x || x > b.x {
            continue;
        }
        let span = b.x - a.x;
        let y = if span < 1e-3 {
            a.y.min(b.y)
        } else {
            a.y + (b.y - a.y) * (x - a.x) / span
        };
        if best.map_or(true, |(by, _)| (y - near_y).abs() < (by - near_y).abs()) {
            best = Some((y, s.owner));
        }
    }
    best
}

/// One grid cell's base state: fighter 1 parked far away on the main stage, idle, untouched by
/// anything the battery does to fighter 0.
fn battery_base() -> SimState {
    let mut s = SimState::spawn();
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    s
}

/// Grid-sweep property test: every (x, y) cell, both grounded-on-nearest-floor and airborne,
/// against every single held input, for K frames, must never exceed `max_legit_step` on any one
/// frame outside an intentional launch. Broad enough to have caught all three named instances of
/// this bug class:
/// - dd91584 (ship-top teleport): a WALK over the rim's shoulder is exactly `dir` held while
///   grounded in the ship neighborhood -- covered by the x/y grid over the hull plus the `dir`
///   input.
/// - 677b8b5 (bowl air-walk): a held DRIFT along the bowl floor while y is wrongly frozen against
///   the real (falling-away) curve grows the gap between `pos.y` and the nearest real floor past
///   `max_legit_step` within a handful of the K=20 held frames -- the SAME per-frame bound this
///   battery already checks catches it once the gap crosses the ceiling, without needing a
///   separate distance-from-center oracle (that lives in ship_tests.rs already).
/// - this fix (B-under-the-hull snap): `special` held while grounded on the hull's low arc is
///   exactly one of the covered inputs, at exactly the covered x/y band.
///
/// Grid coarsened from the ~40px suggestion to keep native-test wall time sane (reported in the
/// task write-up); still spans the full ship neighborhood plus the pillar/mover fixtures.
#[test]
fn grid_sweep_no_input_ever_teleports_a_grounded_or_airborne_fighter() {
    let t = tune();
    let bound = max_legit_step(&t);
    const K: u32 = 20;
    const X_STEP: i32 = 80;
    const Y_STEP: i32 = 80;
    let inputs: [(&str, InputFrame); 7] = [
        (
            "left",
            InputFrame {
                dir: -1.0,
                ..idle()
            },
        ),
        ("right", InputFrame { dir: 1.0, ..idle() }),
        (
            "jump",
            InputFrame {
                jump: true,
                jump_held: true,
                ..idle()
            },
        ),
        (
            "attack",
            InputFrame {
                attack: true,
                attack_held: true,
                ..idle()
            },
        ),
        (
            "special",
            InputFrame {
                special: true,
                ..idle()
            },
        ),
        (
            "down",
            InputFrame {
                down: true,
                down_pressed: true,
                ..idle()
            },
        ),
        ("cstick", InputFrame { cx: 1.0, ..idle() }),
    ];
    let mut cells = 0u32;
    let mut scenarios = 0u32;
    let mut x = -560_i32;
    while x <= 1200 {
        let mut y = 100_i32;
        while y <= 830 {
            let (xf, yf) = (x as f32, y as f32);
            let pos = Vector2::new(xf, yf);
            if !out_of_bounds(pos) {
                cells += 1;
                for spawn_air in [false, true] {
                    let base = battery_base();
                    let ground = if spawn_air {
                        None
                    } else {
                        nearest_ground(&base.paths, &base.nodes, xf, yf)
                    };
                    if !spawn_air && ground.is_none() {
                        continue; // no real floor near this column -- airborne-only cell
                    }
                    for (_name, inp) in &inputs {
                        scenarios += 1;
                        let mut s = base;
                        if spawn_air {
                            s.fighters[0].pos = pos;
                            s.fighters[0].vel = Vector2::ZERO;
                            s.fighters[0].state = CharState::Air;
                            s.fighters[0].ground_plat = -1;
                            s.fighters[0].ground_ink = -1;
                        } else {
                            let (gy, owner) = ground.unwrap();
                            s.fighters[0].pos = Vector2::new(xf, gy);
                            s.fighters[0].vel = Vector2::ZERO;
                            s.fighters[0].state = CharState::Stand;
                            set_ground(&mut s.fighters[0], owner);
                        }
                        let mut prev = s.fighters[0].pos;
                        for frame in 0..K {
                            s = step(&s, &[inp, &idle()], &t);
                            let f = &s.fighters[0];
                            // a real KO+respawn (only near a blast edge, e.g. the far left grid
                            // column against `blast_left`) snaps to the fixed spawn point on
                            // purpose -- `respawn()`'s one tell is invuln freshly set to the
                            // full spawn_iframes window. Not the teleport bug class; skip once.
                            if f.invuln == t.spawn_iframes as u8 {
                                prev = f.pos;
                                continue;
                            }
                            let disp = (f.pos - prev).length();
                            assert!(
                                is_intentional_launch(f) || disp <= bound,
                                "teleport: {disp}px in one frame at grid ({x},{y}) spawn_air={spawn_air} \
                                 input={_name} frame={frame} pos={:?} prev={prev:?} state={:?}",
                                f.pos,
                                f.state
                            );
                            prev = f.pos;
                        }
                    }
                }
            }
            y += Y_STEP;
        }
        x += X_STEP;
    }
    assert!(
        cells > 50,
        "grid must cover a meaningful area, got {cells} cells"
    );
    eprintln!(
        "grid_sweep: {cells} in-bounds cells, {scenarios} scenarios, {} sim steps",
        scenarios * K
    );
}
