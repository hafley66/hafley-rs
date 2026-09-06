//! Pure-sim tests + a deterministic replay harness. `step` is state×inputs→state with no I/O, so
//! the whole sim is a function we can drive frame-by-frame and assert against. The replay harness
//! doubles as a determinism oracle independent of ggrs: drive a scripted input log twice and the
//! per-frame trace must be bit-identical. Capturing a real session's input log (NetInput stream,
//! which already round-trips) and replaying it here is the regression/replay-validation path.

use crate::v1::*;

// --- input builders -----------------------------------------------------------------------------

/// Neutral frame (no buttons, centered stick).
fn idle() -> InputFrame {
    InputFrame::default()
}

/// Build a frame by mutating the neutral default — `press(|i| i.attack = true)`.
fn press(f: impl FnOnce(&mut InputFrame)) -> InputFrame {
    let mut i = InputFrame::default();
    f(&mut i);
    i
}

/// P1 frame + neutral P2.
fn solo(i: InputFrame) -> [InputFrame; 2] {
    [i, InputFrame::default()]
}

/// Build a classified (non-finalized) stroke into `nodes`/`free` from world points, `pos` ZERO so
/// local == world -- the external-crate analogue of the old `p.pts[i] = w; classify(&mut p)`, now
/// that `InkPath` is a handle into the shared `SimState.nodes` arena.
fn build_stroke(
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
    props: StrokeProps,
    owner: i8,
    pts: &[Vector2],
) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.props = props;
    p.owner = owner;
    p.start = free.alloc(pts.len() as u16).unwrap();
    for (i, w) in pts.iter().enumerate() {
        nodes[p.start as usize + i].pt = *w;
    }
    p.len = pts.len() as u8;
    classify(&p, nodes);
    p
}

// --- replay harness -----------------------------------------------------------------------------

/// One frame's observable scalars for both fighters. Enough to catch any divergence without
/// requiring `PartialEq` on the whole `SimState` (whose f32s would make NaN-equality brittle; normal
/// play produces none). `state as u8` is valid: `CharState` is a fieldless enum.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Snap {
    tick: u64,
    state: [u8; 2],
    px: [f32; 2],
    py: [f32; 2],
    dmg: [f32; 2],
    holding: [i8; 2],
}

fn snap(s: &SimState) -> Snap {
    let f = &s.fighters;
    Snap {
        tick: s.tick,
        state: [f[0].state as u8, f[1].state as u8],
        px: [f[0].pos.x, f[1].pos.x],
        py: [f[0].pos.y, f[1].pos.y],
        dmg: [f[0].damage, f[1].damage],
        holding: [f[0].holding, f[1].holding],
    }
}

/// Run a scripted input log from a fresh spawn under default tuning; return the per-frame trace.
fn drive(script: &[[InputFrame; 2]]) -> Vec<Snap> {
    let t = Tune::default();
    let mut s = SimState::spawn();
    let mut trace = Vec::with_capacity(script.len());
    for inputs in script {
        s = step(&s, &[&inputs[0], &inputs[1]], &t);
        trace.push(snap(&s));
    }
    trace
}

/// Fighters spawn airborne (drop-in). Run neutral input until they land and settle, so behavior
/// tests start from a known grounded `Stand`.
fn settled() -> (SimState, Tune) {
    let t = Tune::default();
    let mut s = SimState::spawn();
    for _ in 0..120 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].state,
        CharState::Stand,
        "fighter should settle to Stand"
    );
    (s, t)
}

/// A varied script that exercises movement, jumping, and an attack — the determinism fixture.
fn mixed_script() -> Vec<[InputFrame; 2]> {
    let mut v = Vec::new();
    for _ in 0..20 {
        v.push(solo(press(|i| i.dir = 1.0))); // walk right
    }
    for _ in 0..30 {
        v.push(solo(press(|i| {
            i.jump = true;
            i.jump_held = true;
            i.dir = -1.0;
        }))); // jump + drift left
    }
    v.push(solo(press(|i| i.attack = true))); // swing
    for _ in 0..20 {
        v.push(solo(idle())); // settle
    }
    v
}

// --- determinism oracle -------------------------------------------------------------------------

#[test]
fn replay_is_deterministic() {
    let script = mixed_script();
    let a = drive(&script);
    let b = drive(&script);
    assert_eq!(a, b, "same input log must produce an identical trace");
    assert!(
        a.iter()
            .all(|s| s.px.iter().chain(&s.py).all(|v| v.is_finite())),
        "sim produced a non-finite position"
    );
}

#[test]
fn neutral_input_settles_on_the_floor() {
    let trace = drive(&vec![solo(idle()); 150]);
    let a = trace[trace.len() - 2];
    let b = trace[trace.len() - 1];
    // ignore `tick` (a free-running counter); everything physical should be at a fixed point.
    assert_eq!(
        (a.state, a.px, a.py, a.dmg, a.holding),
        (b.state, b.px, b.py, b.dmg, b.holding),
        "with neutral input the sim should reach a fixed point"
    );
    assert_eq!(b.state[0], CharState::Stand as u8, "settles into Stand");
}

// --- behavior units -----------------------------------------------------------------------------

#[test]
fn jump_leaves_the_ground() {
    let (mut s, t) = settled();
    let ground = s.fighters[0].pos.y;
    // full hop: press + hold for the jumpsquat, then keep holding through takeoff.
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    let mut lowest = s.fighters[0].pos.y;
    for _ in 0..40 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
        lowest = lowest.min(s.fighters[0].pos.y);
    }
    assert!(
        lowest < ground - 50.0,
        "fighter should rise well above the floor (got {lowest} vs {ground})"
    );
}

// These three lock the buffer feel the unit tests above don't reach: the auto-short-hop aerial
// (jump+attack held), the air jump, and the wavedash. They are the golden coverage for the
// Action-model refactor — behavior must be identical before and after.

#[test]
fn jump_plus_attack_autohops_into_an_aerial() {
    let (mut s, t) = settled();
    // same-frame jump + attack with empty hands = auto short-hop aerial
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    let mut saw_aerial = false;
    for _ in 0..14 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
        if matches!(s.fighters[0].state, CharState::Nair | CharState::Dair) {
            saw_aerial = true;
            break;
        }
    }
    assert!(saw_aerial, "jump+attack should auto-hop into an aerial");
    assert!(
        s.fighters[0].autohop_aerial,
        "the auto-hop aerial should be tagged for reduced damage"
    );
}

#[test]
fn second_jump_in_air_is_an_air_jump() {
    let (mut s, t) = settled();
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    for _ in 0..8 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].state,
        CharState::Air,
        "should be airborne after the hop"
    );
    let before = s.fighters[0].air_jumps;
    s = step(&s, &[&idle(), &idle()], &t); // release
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].air_jumps,
        before - 1,
        "the second jump should spend an air jump"
    );
    assert!(
        s.fighters[0].vel.y < 0.0,
        "the air jump should drive the fighter upward"
    );
}

#[test]
fn airdodge_into_the_ground_wavedashes() {
    let (mut s, t) = settled();
    // jump, then airdodge down-toward during the jumpsquat = wavedash out of the squat
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    s = step(
        &s,
        &[
            &press(|i| {
                i.shield_pressed = true;
                i.dir = 1.0;
                i.aim_y = 1.0; // down-forward (screen y is positive downward)
            }),
            &idle(),
        ],
        &t,
    );
    let mut grounded_with_slide = false;
    for _ in 0..20 {
        s = step(&s, &[&idle(), &idle()], &t);
        let f = &s.fighters[0];
        if !matches!(
            f.state,
            CharState::Air | CharState::AirDodge | CharState::JumpSquat
        ) && f.vel.x.abs() > 1.0
        {
            grounded_with_slide = true;
            break;
        }
    }
    assert!(
        grounded_with_slide,
        "an airdodge into the floor should slide along the ground"
    );
}

#[test]
fn grounded_attack_enters_jab() {
    let (s, t) = settled();
    let after = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    assert_eq!(
        after.fighters[0].state,
        CharState::Jab,
        "grounded attack should start a jab"
    );
}

#[test]
fn down_plus_attack_enters_dtilt() {
    let (s, t) = settled();
    let after = step(
        &s,
        &[
            &press(|i| {
                i.attack = true;
                i.down = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        after.fighters[0].state,
        CharState::Dtilt,
        "down + attack on the ground should start the dtilt pothole"
    );
}

/// Land P1 straight down onto the left SOFT platform (index 1) and settle to Stand there, so the
/// soft-platform drop-buffer tests start from a known "crouch-able on a soft platform" pose.
fn on_soft_platform() -> (SimState, Tune) {
    let t = Tune::default();
    let mut s = SimState::spawn();
    s.fighters[0].pos.x = 410.0; // center of PLATFORMS[1] (left soft, x 280..540, y 575)
    s.fighters[0].pos.y = 480.0; // above the platform top, below the top-center platform's x-range
    s.fighters[0].vel.x = 0.0;
    s.fighters[0].vel.y = 0.0;
    s.fighters[0].state = CharState::Air;
    for _ in 0..60 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].state,
        CharState::Stand,
        "should land + settle on the soft platform"
    );
    assert_eq!(
        s.fighters[0].ground_plat, 1,
        "should be standing on the left soft platform"
    );
    (s, t)
}

#[test]
fn down_tap_drops_through_a_soft_platform() {
    let (mut s, t) = on_soft_platform();
    // A single Down TAP (down_pressed on the first frame only) crouches + arms the drop buffer,
    // then drops through within plat_drop_window frames — no re-tap required (Melee/PM feel).
    let mut dropped = false;
    for f in 0..(t.plat_drop_window as usize + 4) {
        let tap = f == 0; // rising edge only on the first frame
        s = step(
            &s,
            &[
                &press(|i| {
                    i.down = true;
                    i.down_pressed = tap;
                }),
                &idle(),
            ],
            &t,
        );
        if s.fighters[0].state == CharState::Air && s.fighters[0].ground_plat < 0 {
            dropped = true;
            break;
        }
    }
    assert!(
        dropped,
        "a Down tap on a soft platform should drop through within the tilt-window"
    );
}

#[test]
fn holding_down_drops_even_without_a_press_edge() {
    // Regression: the shell only fires down_pressed for the digital ui_down action, so a controller
    // or touch stick sets down (held) but never the edge. The drop must still fire off the held bit.
    let (mut s, t) = on_soft_platform();
    let mut dropped = false;
    for _ in 0..(t.plat_drop_window as usize + 6) {
        s = step(&s, &[&press(|i| i.down = true), &idle()], &t); // down HELD, down_pressed never set
        if s.fighters[0].state == CharState::Air && s.fighters[0].ground_plat < 0 {
            dropped = true;
            break;
        }
    }
    assert!(
        dropped,
        "holding Down on a soft platform must drop even if down_pressed never fired"
    );
}

#[test]
fn down_attack_in_the_window_dtilts_instead_of_dropping() {
    let (mut s, t) = on_soft_platform();
    // Frame 0: the Down tap crouches and arms the buffer — it must NOT drop yet.
    s = step(
        &s,
        &[
            &press(|i| {
                i.down = true;
                i.down_pressed = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Crouch,
        "the Down tap crouches first, no instant drop"
    );
    assert_eq!(
        s.fighters[0].ground_plat, 1,
        "still on the platform after the entry tap"
    );
    // Frame 1 (inside plat_drop_window): Down + Attack converts to a Dtilt and cancels the drop.
    s = step(
        &s,
        &[
            &press(|i| {
                i.down = true;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Dtilt,
        "Down+Attack in the window should dtilt"
    );
    assert_eq!(
        s.fighters[0].ground_plat, 1,
        "the dtilt must NOT drop through the platform"
    );
}

#[test]
fn classify_caches_floor_wall_and_grabbable_lip() {
    // a flat shelf (0,0)->(100,0) then a sharp drop (100,0)->(120,200).
    let mut nodes = [InkNode::ZERO; NODE_POOL];
    let mut free = FreeSpans::new();
    let p = build_stroke(
        &mut nodes,
        &mut free,
        StrokeProps::PEN,
        -1,
        &[
            Vector2::new(0.0, 0.0),
            Vector2::new(100.0, 0.0),
            Vector2::new(120.0, 200.0),
        ],
    );
    assert_eq!(
        p.seg_class(0, &nodes),
        SegClass::Ledge,
        "the flat open-end shelf is a grabbable lip"
    );
    assert_eq!(
        p.seg_class(1, &nodes),
        SegClass::Wall,
        "the steep drop classifies as a wall"
    );
}

#[test]
fn fighter_lands_and_stands_on_a_drawn_ink_floor() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    // drop fighter 0 over open air (no soft platform spans x=180; main floor is far below at 760).
    s.fighters[0].pos = Vector2::new(180.0, 250.0);
    // a flat finalized ink shelf directly under the drop, well above the main floor.
    let shelf = build_stroke(
        &mut s.nodes,
        &mut s.free,
        StrokeProps::PEN,
        0,
        &[Vector2::new(100.0, 400.0), Vector2::new(260.0, 400.0)],
    ); // caches Floor/Ledge so collision can read it
    assert!(
        matches!(
            shelf.seg_class(0, &s.nodes),
            SegClass::Floor | SegClass::Ledge
        ),
        "flat shelf is walkable"
    );
    s.paths[0] = shelf;
    // fighter 0 now drops straight down onto the shelf and settles.
    for _ in 0..120 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    let f = &s.fighters[0];
    assert_eq!(
        f.ground_ink, 0,
        "should be standing on the ink path, not fallen through"
    );
    assert_eq!(f.state, CharState::Stand, "settles into Stand on the ink");
    assert!(
        (f.pos.y - 400.0).abs() < 2.0,
        "feet pinned to the ink surface (400), got {}",
        f.pos.y
    );
}

#[test]
fn a_drawn_ink_wall_blocks_horizontal_movement() {
    let (mut s, t) = settled();
    let start = s.fighters[0].pos;
    let wall_x = start.x + 60.0; // just to the right, within a few walk-frames
    // a near-vertical finalized ink stroke spanning the fighter's torso (feet at start.y, ECB ~140 tall).
    let wall = build_stroke(
        &mut s.nodes,
        &mut s.free,
        StrokeProps::PEN,
        0,
        &[
            Vector2::new(wall_x, start.y - 200.0),
            Vector2::new(wall_x, start.y + 40.0),
        ],
    );
    assert_eq!(
        wall.seg_class(0, &s.nodes),
        SegClass::Wall,
        "vertical stroke classifies as a wall"
    );
    s.paths[0] = wall;
    // walk right into the wall for a while; the ECB right vert (38px) must stop at wall_x.
    for _ in 0..40 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    let f = &s.fighters[0];
    assert!(
        f.pos.x <= wall_x - 38.0 + 1.0,
        "fighter should be pinned left of the ink wall (wall {wall_x}, got {})",
        f.pos.x
    );
    assert!(
        f.pos.x > start.x,
        "but should have walked right up to the wall, not stayed put"
    );
}

/// Same vertical stroke as `a_drawn_ink_wall_blocks_horizontal_movement`, but gated one-way
/// (plans/body-unify.md step 5). `gate` picks the orientation; `expect_pass` is whether walking
/// RIGHT into it from the left should pass clean through (true) or block like a plain wall (false).
fn one_way_wall_walk(gate: GateSide, expect_pass: bool) {
    let (mut s, t) = settled();
    let start = s.fighters[0].pos;
    let wall_x = start.x + 60.0;
    let wall = build_stroke(
        &mut s.nodes,
        &mut s.free,
        StrokeProps {
            gate_side: gate,
            ..StrokeProps::PEN
        },
        0,
        &[
            Vector2::new(wall_x, start.y - 200.0),
            Vector2::new(wall_x, start.y + 40.0),
        ],
    );
    assert_eq!(
        wall.seg_class(0, &s.nodes),
        SegClass::Wall,
        "vertical stroke classifies as a wall"
    );
    s.paths[0] = wall;
    for _ in 0..40 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    let f = &s.fighters[0];
    if expect_pass {
        assert!(
            f.pos.x > wall_x + 10.0,
            "gated the pass direction: must walk clean through, got x={}",
            f.pos.x
        );
    } else {
        assert!(
            f.pos.x <= wall_x - 38.0 + 1.0,
            "gated AGAINST the pass direction: must block like a plain wall, got x={}",
            f.pos.x
        );
    }
}

#[test]
fn one_way_ink_passes_a_fighter_moving_with_the_pass_direction() {
    // PassBackward makes rightward the pass direction for this segment's orientation (a top-to-
    // bottom drawn wall): see body::contact::tests for the underlying sign derivation.
    one_way_wall_walk(GateSide::PassBackward, true);
}

#[test]
fn one_way_ink_walls_a_fighter_moving_against_the_pass_direction() {
    one_way_wall_walk(GateSide::PassForward, false);
}

#[test]
fn stroke_registry_resolves_by_id_and_falls_back_to_default() {
    // probe with force_wall: no built-in preset sets it, so it marks row 1 unambiguously
    // (solid stopped distinguishing rows when PEN itself became solid).
    let mut reg = StrokeRegistry::DEFAULT;
    reg.presets[1] = StrokeProps {
        force_wall: true,
        ..StrokeProps::PEN
    };
    assert!(reg.get(1).force_wall, "id 1 resolves to its own preset");
    assert!(!reg.get(0).force_wall, "row 0 is the untouched default");
    assert!(
        !reg.get(99).force_wall,
        "an out-of-range id falls back to the default row"
    );
}

#[test]
fn a_pen_stamps_its_registry_preset_onto_the_path() {
    let (mut s, mut t) = settled();
    // preset row 2 differs from the default (solid); the pen selects it via StrokeId.
    t.strokes.presets[2] = StrokeProps {
        solid: true,
        ..StrokeProps::PEN
    };
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        owner: 0,
        gas: t.ink_budget,
        gas_max: t.ink_budget,
        stroke: 2,
        ..Item::EMPTY
    };
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t); // toggle paint ON
    for _ in 0..20 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    let path = s
        .paths
        .iter()
        .find(|p| p.active() && p.owner == 0)
        .expect("pen laid a path");
    assert!(
        path.props.solid,
        "the path should inherit preset 2's material (solid), not the default"
    );
}

#[test]
fn holding_a_pen_and_attacking_lays_an_ink_path() {
    let (mut s, t) = settled();
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        owner: 0,
        gas: t.ink_budget,
        gas_max: t.ink_budget,
        ..Item::EMPTY
    };
    // tap attack to toggle paint ON, then walk right; the trail pen lays nodes along
    // the movement with the button UP (jump/special stay reachable while painting).
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    for _ in 0..40 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    let path = s.paths.iter().find(|p| p.active() && p.owner == 0);
    assert!(
        path.is_some(),
        "holding a pen and attacking should lay an ink path"
    );
    assert!(
        path.unwrap().len >= 2,
        "a moving trail pen should plant multiple nodes"
    );
}

#[test]
fn attack_over_gun_picks_it_up() {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos, // overlap the body
        vel: Vector2::ZERO,
        owner: -1,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    let after = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    assert_eq!(
        after.fighters[0].holding, 0,
        "attack over an unowned gun should pick it up"
    );
    assert_ne!(
        after.fighters[0].state,
        CharState::Jab,
        "pickup should not also jab"
    );
}

#[test]
fn grab_over_an_item_picks_it_up() {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos, // standing over it
        vel: Vector2::ZERO,
        owner: -1,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    let after = step(&s, &[&press(|i| i.grab = true), &idle()], &t);
    assert_eq!(
        after.fighters[0].holding, 0,
        "grab over an unowned item should pick it up"
    );
    assert_ne!(
        after.fighters[0].state,
        CharState::Grab,
        "item grab should not start a fighter-grab"
    );
}

#[test]
fn firing_a_held_gun_spawns_a_bolt_and_spends_ammo() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos,
        vel: Vector2::ZERO,
        owner: 0,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    let after = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    let bolts = after
        .items
        .iter()
        .filter(|x| x.kind == ItemKind::LaserBolt)
        .count();
    assert_eq!(bolts, 1, "one bolt should spawn");
    assert_eq!(after.items[0].gas, 15.0, "gas should decrement by one shot");
}

#[test]
fn grab_drops_a_held_gun() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos,
        vel: Vector2::ZERO,
        owner: 0,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    let mut after = step(&s, &[&press(|i| i.grab = true), &idle()], &t);
    for _ in 0..t.plat_drop_window + 1 {
        after = step(&after, &[&idle(), &idle()], &t); // ride out the throw-gate window
    }
    assert_eq!(
        after.fighters[0].holding, -1,
        "grab should drop the held item"
    );
    assert!(after.items[0].owner < 0, "dropped gun becomes unowned");
}

#[test]
fn falling_past_the_blast_zone_respawns() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    let spawn_y = s.fighters[0].pos.y;
    s.fighters[0].pos.y = 5000.0; // way past BLAST_Y
    s.fighters[0].damage = 88.0;
    let after = step(&s, &[&idle(), &idle()], &t);
    assert!(
        after.fighters[0].pos.y <= spawn_y + 1.0,
        "should respawn back at the top"
    );
    assert_eq!(after.fighters[0].damage, 0.0, "respawn resets damage");
}

#[test]
fn an_unowned_gun_off_stage_keeps_falling_and_despawns_at_the_blast_zone() {
    // FLOOR_RIGHT = 1050, GROUND_Y = 760, BLAST_Y = 1600 (all pixel space). Place a gun well past
    // the right ledge, above the pit, at rest.
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: Vector2::new(1400.0, 700.0), // off the span (x > FLOOR_RIGHT), above the floor y
        gas: 16.0,
        gas_max: 16.0,
        ..Item::EMPTY
    };
    // one step must NOT snap it to the invisible floor at GROUND_Y (760) — off the span it accrues
    // downward velocity instead of dead-stopping.
    s = step(&s, &[&idle(), &idle()], &t);
    assert!(s.items[0].active(), "still in flight after one step");
    assert!(
        s.items[0].vel.y > 0.0,
        "gravity pulls it down instead of resting on an invisible floor"
    );
    let mut fell_past_floor = false;
    let mut gone = false;
    for _ in 0..600 {
        s = step(&s, &[&idle(), &idle()], &t);
        if s.items[0].active() && s.items[0].pos.y > 760.0 + 1.0 {
            fell_past_floor = true; // dropped below the old invisible floor rather than settling on it
        }
        if !s.items[0].active() {
            gone = true;
            break;
        }
    }
    assert!(
        fell_past_floor,
        "the gun should fall past GROUND_Y (no invisible floor off the span)"
    );
    assert!(gone, "the gun should despawn after crossing the blast zone");
    assert!(!s.items[0].active(), "despawn = empty slot, quietly");
}

#[test]
fn a_bomb_off_stage_despawns_quietly_without_exploding() {
    // A live bomb lobbed over the pit (x well past FLOOR_RIGHT = 1050) must fall past GROUND_Y and
    // cross BLAST_Y (1600) BEFORE its fuse (t.bomb.range) ends, then vanish with NO explosion.
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Bomb,
        pos: Vector2::new(1400.0, 700.0),
        vel: Vector2::ZERO,
        owner: -1,
        timer: t.bomb.range, // full fuse; it should die by falling, not by fusing out
        gas: 0.0,
        gas_max: 1.0,
        ..Item::EMPTY
    };
    let dmg0 = [s.fighters[0].damage, s.fighters[1].damage];
    // first step: it arcs down, it does not detonate on the invisible floor off the span.
    s = step(&s, &[&idle(), &idle()], &t);
    assert!(
        s.items[0].active(),
        "bomb still falling after one step (no invisible-floor detonation)"
    );
    assert!(
        s.items[0].pos.y > 700.0,
        "bomb falls off the span instead of resting/detonating"
    );
    let mut gone = false;
    for _ in 0..600 {
        s = step(&s, &[&idle(), &idle()], &t);
        if !s.items[0].active() {
            gone = true;
            break;
        }
    }
    assert!(
        gone,
        "the bomb should despawn after crossing the blast zone"
    );
    assert!(!s.items[0].active(), "quiet despawn = empty slot");
    assert_eq!(
        [s.fighters[0].damage, s.fighters[1].damage],
        dmg0,
        "a bomb that fell out of bounds must not explode or damage anyone"
    );
}

#[test]
fn dair_hitboxes_are_scaled_up() {
    let t = Tune::default();
    let b = t.dair.boxes;
    assert_eq!(b[0].r, 54.0, "dair box 0 radius scaled 36 -> 54");
    assert_eq!(b[1].r, 57.0, "dair box 1 radius scaled 38 -> 57");
    assert_eq!(b[2].r, 60.0, "dair box 2 radius scaled 40 -> 60");
    assert_eq!(b[3].r, 54.0, "dair sourspot radius scaled 36 -> 54");
}

#[test]
fn an_empty_pen_settles_on_the_ground_then_despawns() {
    // GROUND_Y = 760. An UNOWNED empty pen (gas < 1) dropped above the stage should fall, settle on
    // the floor, and unload (despawn) — unlike a spent gun, which vanishes instantly.
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        pos: Vector2::new(600.0, 760.0 - 60.0), // over the stage, above the floor
        owner: -1,
        gas: 0.0, // out of ink
        gas_max: t.ink_budget,
        ..Item::EMPTY
    };
    let mut gone = false;
    for _ in 0..120 {
        s = step(&s, &[&idle(), &idle()], &t);
        if !s.items[0].active() {
            gone = true;
            break;
        }
    }
    assert!(
        gone,
        "an empty pen should despawn once it settles idle on the ground"
    );

    // a FULL pen resting on the floor must NOT unload — only an empty one does.
    let (mut s2, t2) = settled();
    s2.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        pos: Vector2::new(400.0, 760.0 - 60.0),
        owner: -1,
        gas: t2.ink_budget, // full
        gas_max: t2.ink_budget,
        ..Item::EMPTY
    };
    for _ in 0..120 {
        s2 = step(&s2, &[&idle(), &idle()], &t2);
    }
    assert!(
        s2.items[0].active(),
        "a full pen resting on the ground stays (only an empty pen unloads)"
    );
    assert_eq!(s2.items[0].pos.y, 760.0, "the full pen rests on the floor");
}

#[test]
fn throwing_a_held_item_knocks_back_a_victim() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    // thrower grounded holding a gun; victim standing in front at chest height.
    s.fighters[0].pos = Vector2::new(600.0, 760.0);
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].ground_plat = 0;
    s.fighters[0].facing = 1.0;
    s.fighters[0].holding = 0;
    s.fighters[1].pos = Vector2::new(950.0, 760.0);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: Vector2::new(600.0, 760.0 - 70.0), // chest height, in the flight path
        owner: 0,
        gas: 16.0,
        gas_max: 16.0,
        ..Item::EMPTY
    };
    let dmg0 = s.fighters[1].damage;
    // grab + forward stick = a forward throw (not the neutral soft toss).
    let throw = press(|i| {
        i.grab = true;
        i.dir = 1.0;
    });
    s = step(&s, &[&throw, &idle()], &t);
    assert!(
        s.items[0].thrown,
        "grab + a stick direction arms the held item as a throw"
    );
    assert!(s.fighters[0].holding < 0, "the thrower released the item");
    // let it fly into the victim; it should damage + knock them back, then despawn.
    let mut hit = false;
    for _ in 0..30 {
        s = step(&s, &[&idle(), &idle()], &t);
        if s.fighters[1].damage > dmg0 {
            hit = true;
            break;
        }
    }
    assert!(
        hit,
        "a thrown item should damage + knock back the victim it hits"
    );
    assert!(
        s.fighters[1].hitstun > 0 || s.fighters[1].vel.length() > 1.0,
        "victim took knockback"
    );
}

#[test]
fn a_neutral_grab_while_holding_soft_tosses_instead_of_throwing() {
    let t = Tune::default();
    let mut s = SimState::spawn();
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos,
        owner: 0,
        gas: 16.0,
        gas_max: 16.0,
        ..Item::EMPTY
    };
    // neutral grab (no stick): the throw gate holds the window open, then the gentle
    // drop comes out — NOT an armed throw.
    let mut after = step(&s, &[&press(|i| i.grab = true), &idle()], &t);
    for _ in 0..t.plat_drop_window + 1 {
        after = step(&after, &[&idle(), &idle()], &t);
    }
    assert!(
        after.fighters[0].holding < 0,
        "neutral grab drops the held item once the flick window expires"
    );
    assert!(
        !after.items[0].thrown,
        "a neutral grab is the soft toss, not an armed throw"
    );
    assert!(after.items[0].owner < 0, "soft-tossed item is unowned");
}

#[test]
fn a_flick_just_after_the_grab_press_still_up_throws() {
    // the throw gate: grab tapped neutral, direction arrives 2 frames later — same
    // buffered conversion as a dsmash on a soft platform. No frame-perfect alignment.
    let (mut s, t) = settled();
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        owner: 0,
        gas: 100.0,
        gas_max: 100.0,
        ..Item::EMPTY
    };
    s = step(&s, &[&press(|i| i.grab = true), &idle()], &t); // neutral tap
    assert_eq!(s.fighters[0].holding, 0, "the gate holds: no instant drop");
    s = step(&s, &[&idle(), &idle()], &t); // one neutral frame inside the window
    s = step(&s, &[&press(|i| i.aim_y = -1.0), &idle()], &t); // the late up flick
    let it = s.items[0];
    assert!(it.thrown, "the late flick converts to a real throw");
    assert!(it.vel.y < 0.0, "and it goes UP, vel={:?}", it.vel);
}

#[test]
fn a_tumbling_body_bounces_off_the_floor() {
    // A fast tumbling (spiked) body should invert vel.y off the floor and STAY airborne, instead of
    // dead-stopping to Landing. GROUND_Y = 760.
    let (mut s, t) = settled();
    s.fighters[0].pos = Vector2::new(600.0, 760.0 - 30.0); // just above the main floor top
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].tumble = true;
    s.fighters[0].hitstun = 0;
    s.fighters[0].vel = Vector2::new(0.0, t.tumble_speed + 500.0); // hard downward (above threshold)
    let mut bounced = false;
    for _ in 0..30 {
        s = step(&s, &[&idle(), &idle()], &t);
        if s.fighters[0].vel.y < 0.0 {
            bounced = true;
            break;
        }
    }
    assert!(
        bounced,
        "a fast tumbling body should bounce (vel.y flips upward) off the floor"
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Air,
        "it stays airborne after the bounce (no Landing)"
    );
    assert!(
        s.fighters[0].ground_plat < 0,
        "the bounce did not pin it to a platform"
    );
}

#[test]
fn special_on_ink_stays_pinned_to_the_ink() {
    // The old bug: landing on ink sets ground_plat = 0 ("reads as grounded"), so a grounded
    // special pinned pos.y to PLATFORMS[0].y — teleport to the stage floor for the move, then
    // back up when it ended. The special must integrate on the INK surface.
    let (mut s, t) = settled();
    // a settled permanent bar at y=600 in the clear column left of the left platform (x 170..270)
    s.paths[0] = rehydrate_stroke(
        &[Vector2::new(170.0, 600.0), Vector2::new(270.0, 600.0)],
        StrokeRegistry::TETRIS_ROW,
        0,
        &mut s.nodes,
        &mut s.free,
        &t,
    );
    s.fighters[0].pos = Vector2::new(220.0, 560.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    // fall onto the bar
    for _ in 0..90 {
        s = step(&s, &[&idle(), &idle()], &t);
        if s.fighters[0].ground_ink >= 0 && s.fighters[0].state == CharState::Stand {
            break;
        }
    }
    assert_eq!(s.fighters[0].ground_ink, 0, "standing on the drawn bar");
    let surf = s.fighters[0].pos.y;
    assert!(
        (surf - 600.0).abs() < 2.0,
        "feet on the ink surface, got {surf}"
    );
    // press B (neutral special) and ride the whole move out: never leave the bar's surface
    s = step(&s, &[&press(|i| i.special = true), &idle()], &t);
    for _ in 0..60 {
        assert!(
            (s.fighters[0].pos.y - 600.0).abs() < 4.0,
            "special frame teleported off the ink: y = {} (state {:?})",
            s.fighters[0].pos.y,
            s.fighters[0].state
        );
        s = step(&s, &[&idle(), &idle()], &t);
    }
}

#[test]
fn a_jab_strikes_the_ink_it_touches() {
    // Melee hits ink now: the jab's first hitbox, on its start frame, sweeps the drawn strokes.
    // (shake decays over the ride-out frames, so damage on the bar is the durable evidence)
    let (mut s, t) = settled();
    let f = s.fighters[0];
    // park a settled bar exactly across the jab's first box (off (44,-64), r 32)
    let c = f.pos + Vector2::new(44.0 * f.facing, -64.0);
    s.paths[0] = rehydrate_stroke(
        &[c + Vector2::new(-40.0, 0.0), c + Vector2::new(40.0, 0.0)],
        StrokeRegistry::TETRIS_ROW,
        0,
        &mut s.nodes,
        &mut s.free,
        &t,
    );
    assert!(s.paths[0].mass > 0.0, "the bar is a strikeable body");
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    for _ in 0..12 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert!(
        s.paths[0].percent > 0.0,
        "the jab connected with the ink (hp = {})",
        s.paths[0].percent
    );
}

// --- moveset: aerials / tilts / smashes / turnaround / B-reverse --------------------------------

/// Jump, coast a few airborne frames, and return the state for asserting aerial picks.
fn airborne_then(dir: f32, aim_y: f32) -> (SimState, Tune) {
    let (mut s, t) = settled();
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    for _ in 0..10 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
    }
    assert_eq!(s.fighters[0].state, CharState::Air, "should be airborne");
    s = step(
        &s,
        &[
            &press(|i| {
                i.attack = true;
                i.dir = dir;
                i.aim_y = aim_y;
            }),
            &idle(),
        ],
        &t,
    );
    (s, t)
}

#[test]
fn aerials_pick_by_stick_relative_to_facing() {
    // spawn faces +1: forward = right, back = left.
    let (s, _) = airborne_then(1.0, 0.0);
    assert_eq!(s.fighters[0].state, CharState::Fair, "toward facing = fair");
    let (s, _) = airborne_then(-1.0, 0.0);
    assert_eq!(s.fighters[0].state, CharState::Bair, "behind facing = bair");
    assert_eq!(s.fighters[0].facing, 1.0, "bair must NOT turn you around");
    let (s, _) = airborne_then(0.0, -1.0);
    assert_eq!(s.fighters[0].state, CharState::Uair, "up = uair");
    let (s, _) = airborne_then(0.0, 0.0);
    assert_eq!(s.fighters[0].state, CharState::Nair, "neutral = nair");
}

#[test]
fn held_direction_attack_is_a_tilt_not_a_smash() {
    // walk-strength hold (under DASH_THRESH) for long enough that the flick is stale, then attack.
    let (mut s, t) = settled();
    for _ in 0..10 {
        s = step(&s, &[&press(|i| i.dir = 0.4), &idle()], &t);
    }
    s = step(
        &s,
        &[
            &press(|i| {
                i.dir = 0.4;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Ftilt,
        "held direction + attack = ftilt"
    );
}

#[test]
fn held_up_attack_is_utilt() {
    let (mut s, t) = settled();
    for _ in 0..10 {
        s = step(&s, &[&press(|i| i.aim_y = -1.0), &idle()], &t);
    }
    s = step(
        &s,
        &[
            &press(|i| {
                i.aim_y = -1.0;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Utilt,
        "held up + attack = utilt"
    );
}

#[test]
fn fresh_flick_attack_is_a_smash() {
    // same-frame hard flick + attack = fsmash (the smash input), even on a digital stick.
    let (mut s, t) = settled();
    s = step(
        &s,
        &[
            &press(|i| {
                i.dir = -1.0;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Fsmash,
        "flick + attack = fsmash"
    );
    assert_eq!(s.fighters[0].facing, -1.0, "fsmash faces the flick");
}

#[test]
fn fresh_up_flick_attack_is_usmash() {
    let (mut s, t) = settled();
    s = step(
        &s,
        &[
            &press(|i| {
                i.aim_y = -1.0;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Usmash,
        "up-flick + attack = usmash"
    );
}

#[test]
fn cstick_on_the_ground_smashes() {
    let (mut s, t) = settled();
    s = step(&s, &[&press(|i| i.cx = 1.0), &idle()], &t);
    // the flick queued a Strong; the next actionable frame consumes it.
    for _ in 0..2 {
        if s.fighters[0].state == CharState::Fsmash {
            break;
        }
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].state,
        CharState::Fsmash,
        "grounded c-stick = fsmash"
    );
}

#[test]
fn cstick_in_the_air_throws_the_aerial() {
    let (mut s, t) = settled();
    s = step(
        &s,
        &[
            &press(|i| {
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    for _ in 0..10 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
    }
    s = step(&s, &[&press(|i| i.cx = -1.0), &idle()], &t);
    assert_eq!(
        s.fighters[0].state,
        CharState::Bair,
        "c-stick back in the air = bair, main stick untouched"
    );
}

/// `settled()` lands on the small top platform; drop the fighter over the far-left main floor and
/// let it land there, so momentum tests have the whole 900px stage to run on.
#[test]
fn jump_cancel_grab_stays_grounded_and_replays_at_takeoff_boundary() {
    let (mut initial, t) = settled_on_main();
    initial.fighters[0].char_id = 2;
    for grab_tick in 1..=t.jumpsquat + 1 {
        let mut state = initial;
        let mut replay: SimState = bincode::deserialize(&bincode::serialize(&initial).unwrap()).unwrap();
        for tick in 0..90 {
            let input = net::decode(net::encode(&press(|i| {
                i.jump = tick == 0;
                i.jump_held = true;
                i.grab = tick == grab_tick;
            })));
            let before = state.fighters[0].state;
            state = step(&state, &[&input, &idle()], &t);
            replay = step(&replay, &[&input, &idle()], &t);
            let bytes = bincode::serialize(&state).unwrap();
            assert_eq!(bincode::serialize(&replay).unwrap(), bytes, "grab {grab_tick}, tick {tick}");
            if tick == grab_tick {
                assert_eq!(before, if grab_tick <= t.jumpsquat {
                    CharState::JumpSquat
                } else { CharState::Air }, "takeoff boundary at {grab_tick}");
                if before == CharState::JumpSquat {
                    assert_eq!(state.fighters[0].state, CharState::Grab, "grab tick {grab_tick}");
                    assert_eq!(state.fighters[0].pos.y, initial.fighters[0].pos.y);
                } else {
                    assert_eq!(before, CharState::Air);
                    assert_eq!(state.fighters[0].state, CharState::Air);
                }
            }
            if tick == 30 { replay = bincode::deserialize(&bytes).unwrap(); }
        }
    }
}

fn settled_on_main() -> (SimState, Tune) {
    let (mut s, t) = settled();
    s.fighters[0].pos = Vector2::new(200.0, 250.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    for _ in 0..120 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].state,
        CharState::Stand,
        "re-settled on the main floor"
    );
    assert_eq!(s.fighters[0].ground_plat, 0, "on the solid main stage");
    (s, t)
}

#[test]
fn skid_reverse_turns_around_only_at_zero_velocity() {
    let (mut s, t) = settled_on_main();
    // dash right into a full run.
    for _ in 0..25 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    assert!(s.fighters[0].vel.x > 0.0, "running right");
    // slam the stick the other way: must pass through a braking skid, not flip instantly.
    s = step(&s, &[&press(|i| i.dir = -1.0), &idle()], &t);
    assert_eq!(
        s.fighters[0].state,
        CharState::Skid,
        "reverse enters the brake"
    );
    let mut skid_frames = 0;
    for _ in 0..60 {
        if s.fighters[0].state != CharState::Skid {
            break;
        }
        assert!(
            s.fighters[0].facing > 0.0,
            "facing must not flip while still sliding forward"
        );
        skid_frames += 1;
        s = step(&s, &[&press(|i| i.dir = -1.0), &idle()], &t);
    }
    assert!(
        skid_frames > 3,
        "the brake takes real frames (got {skid_frames})"
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Dash,
        "zero point -> dash out the other way"
    );
    assert_eq!(s.fighters[0].facing, -1.0);
    assert!(s.fighters[0].vel.x < 0.0, "fresh burst goes the new way");
}

#[test]
fn b_reverse_flips_facing_and_mirrors_momentum() {
    let (mut s, t) = settled();
    // dash-jump right for real forward momentum.
    for _ in 0..10 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    s = step(
        &s,
        &[
            &press(|i| {
                i.dir = 1.0;
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    for _ in 0..8 {
        s = step(&s, &[&press(|i| i.jump_held = true), &idle()], &t);
    }
    assert_eq!(s.fighters[0].state, CharState::Air);
    let vx = s.fighters[0].vel.x;
    assert!(vx > 0.0, "drifting right into the special");
    // neutral-B, then flick back inside the window: the wavebounce.
    s = step(&s, &[&press(|i| i.special = true), &idle()], &t);
    assert_eq!(s.fighters[0].state, CharState::SpecialN);
    s = step(&s, &[&press(|i| i.dir = -1.0), &idle()], &t);
    let f = &s.fighters[0];
    assert_eq!(f.facing, -1.0, "B-reverse flips facing");
    assert!(
        f.vel.x < 0.0,
        "momentum mirrors (vel.x {} -> {})",
        vx,
        f.vel.x
    );
}

#[test]
fn jump_cancel_usmash_keeps_the_slide() {
    let (mut s, t) = settled_on_main();
    for _ in 0..25 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    // jump out of the run, then attack + up during the squat = JC usmash carrying momentum.
    s = step(
        &s,
        &[
            &press(|i| {
                i.dir = 1.0;
                i.jump = true;
                i.jump_held = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(s.fighters[0].state, CharState::JumpSquat);
    s = step(
        &s,
        &[
            &press(|i| {
                i.attack = true;
                i.aim_y = -1.0;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Usmash,
        "attack+up in squat = JC usmash"
    );
    assert!(
        s.fighters[0].vel.x > 0.0,
        "the usmash slides with run momentum"
    );
}

// --- body-bus behavior changes (plans/body-bus.md step 4) -----------------------------------------

#[test]
fn launched_body_lands_on_a_soft_platform() {
    // Pre-body-bus, hitstun physics knew only GROUND_Y: a launched body fell straight
    // through soft platforms. Now it sweeps the same soup as everyone else.
    // PLATFORMS[1]: left 280, right 540, y 575 (soft).
    let (mut s, t) = settled();
    s.fighters[0].pos = Vector2::new(400.0, 500.0); // above the left soft platform
    s.fighters[0].state = CharState::Launched;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].hitstun = 40;
    s.fighters[0].tumble = false;
    s.fighters[0].vel = Vector2::new(0.0, 300.0); // knocked downward, light (no tumble bounce)
    for _ in 0..60 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert!(
        (s.fighters[0].pos.y - 575.0).abs() < 2.0,
        "launched body should catch the soft platform at y=575, got y={}",
        s.fighters[0].pos.y
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::Stand,
        "recovered standing on it"
    );
}

#[test]
fn wall_touch_refreshes_air_resources() {
    // Any-surf refresh: pressing into the stage's side wall resets jumps + dodges
    // (the wall-hang/climb direction; deliberate body-bus rule).
    let (mut s, t) = settled();
    s.fighters[0].pos = Vector2::new(100.0, 820.0); // beside the left face, below the lip
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].air_jumps = 0;
    s.fighters[0].air_dodges = 0;
    s.fighters[0].vel = Vector2::new(400.0, 0.0); // drifting into the wall
    let mut refreshed = false;
    for _ in 0..20 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
        if s.fighters[0].air_jumps == t.max_air_jumps as u8
            && s.fighters[0].air_dodges == t.max_air_dodges as u8
        {
            refreshed = true;
            break;
        }
    }
    assert!(refreshed, "wall contact should refresh air jumps + dodges");
}

#[test]
fn a_dropped_item_settles_on_a_soft_platform() {
    // Pre-body-bus, free items only knew the main floor span: anything over a platform
    // fell straight through to GROUND_Y. PLATFORMS[1]: left 280, right 540, y 575.
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: Vector2::new(400.0, 500.0), // above the left soft platform
        vel: Vector2::ZERO,
        owner: -1,
        gas: 3.0,
        gas_max: 3.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    for _ in 0..90 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert!(
        (s.items[0].pos.y - 575.0).abs() < 2.0,
        "gun should rest on the platform at y=575, got y={}",
        s.items[0].pos.y
    );
}

#[test]
fn a_bomb_detonates_on_a_platform_top() {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Bomb,
        pos: Vector2::new(700.0, 500.0), // above the right soft platform (660..920 @ 575)
        vel: Vector2::new(0.0, 100.0),
        owner: 0,
        gas: 0.0,
        gas_max: 1.0,
        timer: 600, // fuse far off: only floor contact can pop it in this window
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    let mut popped = false;
    for _ in 0..90 {
        s = step(&s, &[&idle(), &idle()], &t);
        if !s.items[0].active() {
            popped = true;
            break;
        }
        assert!(
            s.items[0].pos.y < 760.0,
            "bomb must not pass the platform toward the main floor"
        );
    }
    assert!(popped, "platform contact should detonate the bomb");
}

#[test]
fn cstick_aims_a_held_gun_and_suppresses_the_smash_macro() {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos,
        vel: Vector2::ZERO,
        owner: 0,
        gas: 5.0,
        gas_max: 5.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    s.fighters[0].holding = 0;
    // c-stick straight up + attack: the shot goes UP, and no Usmash comes out
    s = step(
        &s,
        &[
            &press(|i| {
                i.attack = true;
                i.cy = -1.0; // up
            }),
            &idle(),
        ],
        &t,
    );
    let bolt = s.items.iter().find(|it| it.kind == ItemKind::LaserBolt);
    let bolt = bolt.expect("an aimed shot came out");
    assert!(
        bolt.vel.y < -1000.0 && bolt.vel.x.abs() < 1.0,
        "bolt flies straight up, got {:?}",
        bolt.vel
    );
    assert!(
        !matches!(s.fighters[0].state, CharState::Usmash),
        "the c-stick was a gun sight, not a smash macro"
    );
}

// --- drawn-shot ink gun (plans/body-bus.md step 8) ------------------------------------------------

/// Draws an anchored shape with the ink gun: A-press with neutral c-stick (anchor_dir =
/// facing), then a c-stick sweep to lay nodes around the anchor. Returns the state
/// mid-draw (attack still held).
fn ink_gun_mid_draw() -> (SimState, Tune) {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::InkGun,
        owner: 0,
        gas: t.ink_budget,
        gas_max: t.ink_budget,
        ..Item::EMPTY
    };
    s.fighters[0].holding = 0;
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t); // toggle paint ON
    for k in 0..12 {
        let sweep = press(|i| {
            i.cx = -1.0 + k as f32 / 6.0;
            i.cy = -0.6;
        });
        s = step(&s, &[&sweep, &idle()], &t);
    }
    (s, t)
}

#[test]
fn releasing_an_anchored_draw_fires_it_as_a_traveling_body() {
    let (mut s, t) = ink_gun_mid_draw();
    let mid = s
        .paths
        .iter()
        .find(|p| p.drawing && p.owner == 0)
        .expect("holding A laid an anchored path");
    assert!(mid.anchor >= 0, "the ink gun's draw is anchored");
    assert!(
        mid.len >= 2,
        "the c-stick sweep planted nodes, got {}",
        mid.len
    );
    assert!(
        mid.anchor_dir.x > 0.9,
        "neutral c at press anchors along facing, got {:?}",
        mid.anchor_dir
    );
    // second attack press: the toggle ends the stroke and fires it along the anchor dir
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    let shot = s
        .paths
        .iter()
        .find(|p| p.active() && p.owner == 0)
        .expect("the shape survives release");
    assert!(!shot.drawing && shot.anchor < 0, "release ends the draw");
    assert!(shot.traveling(), "release fires it, vel={:?}", shot.vel);
    assert!(
        shot.vel.x > stage::INK_TRUCK_SPEED,
        "it flies along anchor_dir fast enough to be a hazard, vel={:?}",
        shot.vel
    );
}

#[test]
fn a_mid_draw_hit_rips_the_shape_loose_at_half_mass() {
    let (mut s, t) = ink_gun_mid_draw();
    // clobber the drawer: next frame update_paths sees hitstun and rips the shape free
    s.fighters[0].state = CharState::Launched;
    s.fighters[0].hitstun = 20;
    s = step(&s, &[&idle(), &idle()], &t);
    let ripped = s
        .paths
        .iter()
        .find(|p| p.active() && p.owner == 0)
        .expect("the shape survives the hit");
    assert!(
        !ripped.drawing && ripped.anchor < 0,
        "the hit ends the draw"
    );
    // half mass: finalize computes seg-length x density, the rip halves it
    let n = ripped.len as usize;
    let full: f32 = (0..n - 1)
        .map(|i| {
            let base = ripped.start as usize + i;
            (s.nodes[base + 1].pt - s.nodes[base].pt).length()
        })
        .sum::<f32>()
        * ripped.props.density;
    assert!(
        (ripped.mass - full * 0.5).abs() < 1.0,
        "mass halved: {} vs full {}",
        ripped.mass,
        full
    );
    assert!(
        ripped.vel.x < 0.0,
        "it flies back at its drawer (anchored to the right), vel={:?}",
        ripped.vel
    );
}

// --- the save: buffered airdodge out of hitstun (plans/body-bus.md step 9) ------------------------

/// A launched, airborne body far from any surface, `damage` % in, 10 frames of stun left.
fn launched_high(damage: f32) -> (SimState, Tune) {
    let (mut s, t) = settled();
    s.fighters[0].pos = Vector2::new(600.0, 200.0);
    s.fighters[0].state = CharState::Launched;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].hitstun = 10;
    s.fighters[0].tumble = false;
    s.fighters[0].damage = damage;
    s.fighters[0].vel = Vector2::new(900.0, -300.0);
    (s, t)
}

/// Steps with neutral input until the fighter leaves hitstun (max 15 frames).
fn run_out_the_stun(mut s: SimState, t: &Tune) -> SimState {
    for _ in 0..15 {
        s = step(&s, &[&idle(), &idle()], t);
        if s.fighters[0].hitstun == 0 {
            return s;
        }
    }
    s
}

#[test]
fn a_buffered_shield_press_saves_out_of_hitstun_at_low_percent() {
    let (mut s, t) = launched_high(40.0);
    // shield during stun arms the buffer; the save fires the frame stun expires
    s = step(&s, &[&press(|i| i.shield_pressed = true), &idle()], &t);
    let dodges_before = s.fighters[0].air_dodges;
    s = run_out_the_stun(s, &t);
    let f = s.fighters[0];
    assert_eq!(
        f.state,
        CharState::AirDodge,
        "the save is an air dodge out of stun"
    );
    assert_eq!(f.air_dodges, dodges_before - 1, "it spends a charge");
    assert!(
        f.vel.length() < 1e-3,
        "under save_zero_pct with a neutral stick the launch is fully cancelled, vel={:?}",
        f.vel
    );
}

#[test]
fn a_high_percent_save_keeps_part_of_the_launch() {
    let (mut s, t) = launched_high(200.0); // 100 over the zero point: keep = 100 * 0.005 = half
    s = step(&s, &[&press(|i| i.shield_pressed = true), &idle()], &t);
    s = run_out_the_stun(s, &t);
    let f = s.fighters[0];
    assert_eq!(f.state, CharState::AirDodge, "the save still comes out");
    assert!(
        f.vel.x > 900.0 * 0.3 && f.vel.x < 900.0 * 0.7,
        "about half the launch bleeds through at 200%, vel.x={}",
        f.vel.x
    );
}

#[test]
fn no_dodge_charge_means_no_save() {
    let (mut s, t) = launched_high(40.0);
    s.fighters[0].air_dodges = 0;
    s = step(&s, &[&press(|i| i.shield_pressed = true), &idle()], &t);
    s = run_out_the_stun(s, &t);
    let f = s.fighters[0];
    assert_eq!(
        f.state,
        CharState::Air,
        "spent dodges: stun ends into plain Air"
    );
    assert!(
        f.vel.x > 500.0,
        "the launch momentum survives, vel.x={}",
        f.vel.x
    );
}

// --- item feel: pickup latch, c-stick throw, momentum transfer ------------------------------------

#[test]
fn picking_up_a_gun_does_not_auto_fire_it() {
    let (mut s, t) = settled();
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: s.fighters[0].pos,
        owner: -1,
        gas: 16.0,
        gas_max: 16.0,
        facing: 1.0,
        ..Item::EMPTY
    };
    // press attack over the gun (pickup) and KEEP HOLDING it
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    assert_eq!(s.fighters[0].holding, 0, "the press picked the gun up");
    for _ in 0..20 {
        s = step(&s, &[&press(|i| i.attack_held = true), &idle()], &t);
    }
    assert!(
        !s.items.iter().any(|it| it.kind == ItemKind::LaserBolt),
        "the held-over press from the pickup must not fire"
    );
    // release, then a FRESH press fires
    s = step(&s, &[&idle(), &idle()], &t);
    s = step(&s, &[&press(|i| i.attack = true), &idle()], &t);
    assert!(
        s.items.iter().any(|it| it.kind == ItemKind::LaserBolt),
        "a fresh press after release fires normally"
    );
}

#[test]
fn a_cstick_flick_throws_a_held_item_without_turning() {
    let (mut s, t) = settled();
    s.fighters[0].holding = 0;
    s.fighters[0].facing = 1.0;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        owner: 0,
        gas: 100.0,
        gas_max: 100.0,
        ..Item::EMPTY
    };
    // c-stick LEFT (behind the facing): the item flies left, the fighter never turns
    s = step(&s, &[&press(|i| i.cx = -1.0), &idle()], &t);
    let it = s.items[0];
    assert!(it.thrown, "the c-flick is a throw");
    assert!(
        it.vel.x < 0.0,
        "thrown backward without a turnaround, vel={:?}",
        it.vel
    );
    assert_eq!(
        s.fighters[0].facing, 1.0,
        "the thrower keeps facing forward"
    );
    assert!(
        !matches!(
            s.fighters[0].state,
            CharState::Fsmash | CharState::Usmash | CharState::Dsmash
        ),
        "no smash attack came out of the flick"
    );
}

#[test]
fn a_throw_inherits_the_throwers_momentum() {
    let (mut s, t) = settled();
    s.fighters[0].holding = 0;
    s.fighters[0].vel = Vector2::new(600.0, 0.0); // mid-dash
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::Pen,
        owner: 0,
        gas: 100.0,
        gas_max: 100.0,
        ..Item::EMPTY
    };
    // up-throw with the grab button while moving right
    s = step(
        &s,
        &[
            &press(|i| {
                i.grab = true;
                i.aim_y = -1.0;
            }),
            &idle(),
        ],
        &t,
    );
    let it = s.items[0];
    assert!(it.thrown, "grab + up = up throw");
    assert!(it.vel.y < 0.0, "it goes up");
    assert!(
        it.vel.x > 300.0,
        "the dash momentum rides along (glide toss), vel={:?}",
        it.vel
    );
}

#[test]
fn a_dash_attack_scoops_an_item_it_passes_over() {
    let (mut s, t) = settled_on_main();
    // gun well ahead ON THE SAME FLOOR: an attack pressed now is out of pickup reach, so it
    // starts a dash attack instead of a direct pickup.
    let fy = s.fighters[0].pos.y;
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos: Vector2::new(s.fighters[0].pos.x + 320.0, fy),
        vel: Vector2::ZERO,
        owner: -1,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    // flick into a dash, let it establish past the smash window, then press attack (= dash attack).
    for _ in 0..8 {
        s = step(&s, &[&press(|i| i.dir = 1.0), &idle()], &t);
    }
    s = step(
        &s,
        &[
            &press(|i| {
                i.dir = 1.0;
                i.attack = true;
            }),
            &idle(),
        ],
        &t,
    );
    assert_eq!(
        s.fighters[0].state,
        CharState::DashAttack,
        "an out-of-reach press from a run is the dash attack"
    );
    // the lunge carries the fighter over the gun; the slide should claim it hands-free.
    for _ in 0..40 {
        s = step(&s, &[&idle(), &idle()], &t);
    }
    assert_eq!(
        s.fighters[0].holding, 0,
        "the dash attack should scoop the item on the pass-over frame"
    );
}
#[test]
fn falcon_walk_in_hits_and_replays_every_tick() {
    let tune = Tune::default();
    let mut state = SimState::spawn();
    state.fighters[0].char_id = 2;
    state.fighters[1].char_id = 2;
    let mut replay: SimState = bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
    let mut peak = 0.0_f32;
    for tick in 0..360 {
        // Land, walk together for 24 ticks, then repeat neutral attacks. No state teleport.
        let frames = [0, 1].map(|p| InputFrame {
            // Use a representable wire value above the movement deadzone: 0.25 truncates
            // to 31/127, which is below that threshold after the packet round-trip.
            dir: if (60..84).contains(&tick) { if p == 0 { 32.0 / 127.0 } else { -32.0 / 127.0 } } else { 0.0 },
            attack: tick >= 90 && tick % 30 == 0,
            attack_held: tick >= 90 && tick % 30 < 8,
            ..Default::default()
        });
        let frames = frames.map(|frame| net::decode(net::encode(&frame)));
        state = step(&state, &[&frames[0], &frames[1]], &tune);
        replay = step(&replay, &[&frames[0], &frames[1]], &tune);
        let expected = bincode::serialize(&state).unwrap();
        assert_eq!(bincode::serialize(&replay).unwrap(), expected, "tick {tick}");
        if tick == 179 { replay = bincode::deserialize(&expected).unwrap(); }
        peak = peak.max(state.fighters[0].damage + state.fighters[1].damage);
    }
    assert_eq!(peak, 60.0, "the input sequence must produce actual hit damage");
}
