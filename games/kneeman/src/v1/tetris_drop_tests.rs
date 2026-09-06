// TetrisDropper (queue-2026-07-03 item 2): TetrisGun's sibling -- same permanent TETRIS-row
// piece, same tetromino table, but the shot is a PURE VERTICAL drop in front of the fighter
// instead of an arc lob. Declared as a crate-root child so `super::*` is the crate root (same
// convention as `grab_tests`/`wings_wear_tests`).

use super::*;
use crate::v1::items::tetris_drop::shape_from_aim_y;

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

/// A held TetrisDropper (fighter 0, `facing`, `gas` shots left), standing at (600, GROUND_Y).
/// Seeded directly (owner + holding set, like `grab_tests`' held-item setups) rather than through
/// a real pickup -- `pickup_hold` stays false so the very first attack press fires immediately.
fn holding_dropper(facing: f32, gas: f32) -> SimState {
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].ground_plat = 0;
    s.fighters[0].facing = facing;
    s.fighters[0].holding = 0;
    s.items[0] = Item {
        kind: ItemKind::TetrisDropper,
        pos: Vector2::new(600.0, GROUND_Y - 240.0),
        vel: Vector2::ZERO,
        owner: 0,
        gas,
        gas_max: gas,
        timer: 0,
        facing,
        tool: ToolKind::TrailPen,
        stroke: StrokeRegistry::TETRIS_ROW,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    s
}

/// The path slot the fire just claimed: the one active, owner-0, still-Traveling piece.
fn fired_slot(s: &SimState) -> usize {
    let hits: Vec<usize> = s
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.active() && p.owner == 0 && p.traveling())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one freshly-fired piece, got {hits:?}"
    );
    hits[0]
}

// ── pure drop: x is constant, it lands ahead on the facing side ─────────────────────────────

#[test]
fn drop_falls_with_constant_x_and_lands_in_front_on_the_facing_side() {
    let t = tune();
    let s = holding_dropper(1.0, 3.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let mut s = step(&s, &[&atk, &IDLE], &t);

    let slot = fired_slot(&s);
    let spawn_x = s.paths[slot].pos.x;
    assert!(
        spawn_x > 600.0,
        "facing right: the piece should spawn ahead of the shooter, got x={spawn_x}"
    );

    let mut landed = false;
    for _ in 0..300 {
        if !s.paths[slot].traveling() {
            landed = true;
            break;
        }
        let before_x = s.paths[slot].pos.x;
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert!(
            (s.paths[slot].pos.x - before_x).abs() < 1e-4,
            "x moved mid-fall: {} -> {}",
            before_x,
            s.paths[slot].pos.x
        );
    }
    assert!(landed, "piece never settled within 300 frames");
    assert!(
        (s.paths[slot].pos.x - spawn_x).abs() < 1e-3,
        "landed x drifted from the spawn x: {} vs {}",
        s.paths[slot].pos.x,
        spawn_x
    );
}

#[test]
fn drop_spawns_behind_the_facing_line_when_facing_left() {
    let t = tune();
    let s = holding_dropper(-1.0, 3.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let s = step(&s, &[&atk, &IDLE], &t);
    let slot = fired_slot(&s);
    assert!(
        s.paths[slot].pos.x < 600.0,
        "facing left: the piece should spawn ahead (behind, on screen) of the shooter, got x={}",
        s.paths[slot].pos.x
    );
}

// ── aim_y quantization: stick up/neutral/down picks the expected tetromino ──────────────────

#[test]
fn aim_y_quantizes_up_neutral_down() {
    assert_eq!(shape_from_aim_y(-1.0), 0, "stick up -> the first shape");
    assert_eq!(shape_from_aim_y(0.0), 2, "neutral -> the middle shape");
    assert_eq!(shape_from_aim_y(1.0), 4, "stick down -> the last shape");
    // out-of-range analog reads clamp instead of indexing past the table.
    assert_eq!(shape_from_aim_y(-5.0), 0);
    assert_eq!(shape_from_aim_y(5.0), 4);
}

/// End-to-end: firing with the stick held up actually reaches `fire_gun` and produces the
/// widest, flattest piece (shape 0, the "I" -- 4x1 cells; every other shape is squarer). Proves
/// `aim_y` really flows InputFrame -> Act::Fire -> fire_gun -> shape_from_aim_y, not just that
/// the pure function's math is right.
#[test]
fn fired_piece_follows_aim_y_up_into_the_flat_i_piece() {
    let t = tune();
    let s = holding_dropper(1.0, 3.0);
    let up = InputFrame {
        attack: true,
        aim_y: -1.0,
        ..IDLE
    };
    let s = step(&s, &[&up, &IDLE], &t);
    let slot = fired_slot(&s);
    let p = &s.paths[slot];
    let (mut lo, mut hi) = (p.world_pt(0, &s.nodes), p.world_pt(0, &s.nodes));
    for i in 0..p.len as usize {
        let w = p.world_pt(i, &s.nodes);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    assert!(
        (w - TETRIS_CELL * 4.0).abs() < 1.0 && (h - TETRIS_CELL).abs() < 1.0,
        "stick-up should fire the I piece (4x1 cells): got {w}x{h}"
    );
}

// ── permanent TETRIS material, same as the arc gun's ─────────────────────────────────────────

#[test]
fn dropped_piece_bakes_permanent_tetris_material_on_landing() {
    let t = tune();
    let s = holding_dropper(1.0, 3.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let mut s = step(&s, &[&atk, &IDLE], &t);
    let slot = fired_slot(&s);
    for _ in 0..300 {
        if !s.paths[slot].traveling() {
            break;
        }
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(!s.paths[slot].traveling(), "must have settled by now");
    assert_eq!(
        s.paths[slot].vel,
        Vector2::ZERO,
        "settled ink is locked (Still IS the cluster)"
    );
    assert_eq!(
        s.paths[slot].props,
        StrokeProps::TETRIS,
        "landed piece must resolve the row-1 TETRIS material, same as the arc gun"
    );
}

// ── gas depletes per shot; a spent gun despawns instantly (TetrisGun's semantics) ────────────

#[test]
fn gas_depletes_one_per_shot() {
    let t = tune();
    let s = holding_dropper(1.0, 3.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let s = step(&s, &[&atk, &IDLE], &t);
    assert_eq!(
        s.items[0].kind,
        ItemKind::TetrisDropper,
        "still holding, ammo left"
    );
    assert_eq!(s.items[0].gas, 2.0);
    assert_eq!(s.fighters[0].holding, 0);
}

#[test]
fn spent_dropper_vanishes_instantly_like_the_arc_gun() {
    let t = tune();
    let s = holding_dropper(1.0, 1.0); // last shot loaded
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let s = step(&s, &[&atk, &IDLE], &t);
    assert_eq!(
        s.items[0].kind,
        ItemKind::None,
        "the last shot empties the gun and it vanishes on the spot"
    );
    assert_eq!(s.fighters[0].holding, -1, "the empty hand lets go");
}
