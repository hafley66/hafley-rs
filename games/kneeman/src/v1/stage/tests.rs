// Unit tests for stage.rs (ink paths, classify, blast zone, strike physics).
// Split out of stage/mod.rs; still white-box (super::*) so they reach private helpers.

use super::*;
use crate::v1::arena::{FreeSpans, InkNode, Scratch};

/// Push some world points as a finished (non-drawing) stroke and classify, WITHOUT the
/// finalize rebase — the "old representation" (pos = ZERO, pts world) as a control.
fn raw_path(nodes: &mut [InkNode], free: &mut FreeSpans, world: &[Vector2]) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.owner = 0;
    p.start = free.alloc(world.len() as u16).unwrap();
    for (i, w) in world.iter().enumerate() {
        p.push(*w, i as u64, nodes);
    }
    classify(&p, nodes);
    p
}

/// A small finalized body-stroke centered at `at` (a 60px flat bar), ready to fly.
fn body_at(nodes: &mut [InkNode], free: &mut FreeSpans, at: Vector2) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.owner = 0;
    p.drawing = true;
    p.start = free.alloc(MAX_PATH_PTS as u16).unwrap();
    p.push(at + Vector2::new(-30.0, 0.0), 0, nodes);
    p.push(at + Vector2::new(30.0, 0.0), 1, nodes);
    finalize_path(&mut p, nodes, free);
    p
}

#[test]
fn finalize_rebases_pos_to_centroid_without_moving_world_geometry() {
    let mut sc = Scratch::new();
    let world = [
        Vector2::new(100.0, 50.0),
        Vector2::new(220.0, 64.0),
        Vector2::new(300.0, 40.0),
    ];
    let mut p = InkPath::EMPTY;
    p.owner = 0;
    p.drawing = true;
    p.start = sc.free.alloc(MAX_PATH_PTS as u16).unwrap();
    for (i, w) in world.iter().enumerate() {
        p.push(*w, i as u64, &mut sc.nodes);
    }
    assert_eq!(p.pos, Vector2::ZERO, "while drawing local == world");
    finalize_path(&mut p, &mut sc.nodes, &mut sc.free);
    let c = (world[0] + world[1] + world[2]) / 3.0;
    assert!((p.pos - c).length() < 1e-3, "pos is the node centroid");
    for (i, w) in world.iter().enumerate() {
        assert!(
            (p.world_pt(i, &sc.nodes) - *w).length() < 1e-3,
            "world_pt {i} unmoved by rebase"
        );
    }
}

#[test]
fn rebased_path_collides_identically_to_world_space_path() {
    let mut sc = Scratch::new();
    let world = [
        Vector2::new(100.0, 400.0),
        Vector2::new(260.0, 400.0), // flat floor span
        Vector2::new(262.0, 250.0), // near-vertical wall up
    ];
    let control = raw_path(&mut sc.nodes, &mut sc.free, &world); // pos = ZERO, pts world (the old representation)
    let mut rebased = control;
    // rebased needs its own span (finalize shrinks/frees it); copy control's geometry into a fresh span.
    rebased.start = sc.free.alloc(control.len as u16).unwrap();
    for i in 0..control.len as usize {
        sc.nodes[rebased.start as usize + i] = sc.nodes[control.start as usize + i];
    }
    rebased.drawing = true;
    finalize_path(&mut rebased, &mut sc.nodes, &mut sc.free);
    for x in [100.0f32, 150.0, 200.0, 259.0] {
        let a = ink_floor_y_at(&control, x, &sc.nodes);
        let b = ink_floor_y_at(&rebased, x, &sc.nodes);
        match (a, b) {
            (Some(ya), Some(yb)) => assert!((ya - yb).abs() < 1e-3, "floor y at {x}"),
            (a, b) => assert_eq!(a.is_some(), b.is_some(), "floor presence at {x}"),
        }
    }
    // wall block: approach the near-vertical segment from the left at its mid height
    // (the old ink_wall_block, generalized: each path's Wall surfs through sweep_walls)
    let wall_hit = |p: &InkPath, nodes: &[InkNode]| {
        let mut surfs = Vec::new();
        crate::v1::body::Randall::surfs(p, crate::v1::body::SurfOwner::Ink(0), nodes, &mut |s| {
            surfs.push(s)
        });
        crate::v1::body::sweep_walls(220.0, Vector2::new(258.0, 340.0), 10.0, 20.0, &surfs)
            .map(|w| (w.x, w.nx))
    };
    match (wall_hit(&control, &sc.nodes), wall_hit(&rebased, &sc.nodes)) {
        (Some((xa, na)), Some((xb, nb))) => {
            assert!((xa - xb).abs() < 1e-3);
            assert_eq!(na, nb);
        }
        (a, b) => assert_eq!(a.is_some(), b.is_some(), "wall block presence"),
    }
}

#[test]
fn finalize_computes_mass_from_length_times_density() {
    let mut sc = Scratch::new();
    let world = [
        Vector2::new(0.0, 0.0),
        Vector2::new(100.0, 0.0),
        Vector2::new(100.0, 50.0),
    ];
    let mut p = InkPath::EMPTY;
    p.owner = 0;
    p.drawing = true;
    p.start = sc.free.alloc(MAX_PATH_PTS as u16).unwrap();
    for (i, w) in world.iter().enumerate() {
        p.push(*w, i as u64, &mut sc.nodes);
    }
    assert_eq!(p.mass, 0.0, "no mass while drawing");
    finalize_path(&mut p, &mut sc.nodes, &mut sc.free);
    assert!(
        (p.mass - 150.0 * p.props.density).abs() < 1e-2,
        "mass = length x density, got {}",
        p.mass
    );

    // a zero-density material never becomes a body, even finalized (baked stage preset shape)
    let mut baked = InkPath::EMPTY;
    baked.props.density = 0.0;
    baked.drawing = true;
    baked.start = sc.free.alloc(MAX_PATH_PTS as u16).unwrap();
    for (i, w) in world.iter().enumerate() {
        baked.push(*w, i as u64, &mut sc.nodes);
    }
    finalize_path(&mut baked, &mut sc.nodes, &mut sc.free);
    assert_eq!(baked.mass, 0.0);
    assert!(!baked.traveling());
}

#[test]
fn traveling_ink_arcs_and_locks_on_the_ground() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut p = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 300.0));
    p.vel = Vector2::new(2.0, -4.0); // lobbed up-right
    let before = p.pos;
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    let mut frames = 0;
    while p.traveling() && frames < 1200 {
        integrate_ink(&mut p, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
        frames += 1;
    }
    assert!(!p.traveling(), "ink settled within {frames} frames");
    assert!(p.pos.x > before.x, "carried its horizontal momentum");
    // roll spin can settle the body TILTED with an end hanging past the platform edge, so the
    // guarantee is: the snapped node rests ON a top it spans — not that the whole body is flat.
    let on_a_top = (0..p.len as usize).any(|i| {
        let w = p.world_pt(i, &sc.nodes);
        PLATFORMS
            .iter()
            .any(|pl| (w.y - pl.y).abs() < 1e-2 && w.x >= pl.left && w.x <= pl.right)
    });
    assert!(
        on_a_top,
        "a node sits ON a platform top (pos {:?}, rot {})",
        p.pos, p.rot
    );
}

#[test]
fn off_stage_traveling_ink_falls_to_the_blast_floor_and_dies() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut p = body_at(
        &mut sc.nodes,
        &mut sc.free,
        Vector2::new(FLOOR_RIGHT + 400.0, 300.0),
    ); // past the stage edge
    p.vel = Vector2::new(0.0, 1.0);
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    for _ in 0..2000 {
        integrate_ink(&mut p, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
        if !p.active() {
            break;
        }
    }
    assert!(
        !p.active(),
        "no surface off-stage: ink falls past BLAST_Y and despawns"
    );
}

#[test]
fn blast_zone_is_the_bounding_box_of_still_zone_ink_and_prunes_outsiders() {
    let mut n = crate::v1::SimState::spawn();
    n.paths[SHIP_SLOT] = InkPath::EMPTY; // isolate PLAYER zone ink (the hull is zone material too)
    // zone ink: a bar around x=600 marks the zone
    let mut zone_ink = body_at(&mut n.nodes, &mut n.free, Vector2::new(600.0, 500.0));
    zone_ink.props.zone = true;
    n.paths[0] = zone_ink;
    // pencil ink inside the zone survives; pencil ink far outside dies
    n.paths[1] = body_at(&mut n.nodes, &mut n.free, Vector2::new(600.0, 500.0));
    n.paths[2] = body_at(&mut n.nodes, &mut n.free, Vector2::new(2500.0, 500.0));
    // traveling ink outside is exempt until it settles
    let mut flying = body_at(&mut n.nodes, &mut n.free, Vector2::new(2500.0, 200.0));
    flying.vel = Vector2::new(1.0, 1.0);
    n.paths[3] = flying;

    let (lo, hi) = ink_blast_zone(&n.paths, &n.nodes).expect("zone ink defines a zone");
    assert!(
        (lo.x - 570.0).abs() < 1e-3 && (hi.x - 630.0).abs() < 1e-3,
        "bounding box of the zone bar"
    );

    prune_outside(&mut n);
    assert!(n.paths[1].active(), "still ink inside the zone survives");
    assert!(!n.paths[2].active(), "still ink outside the zone is pruned");
    assert!(n.paths[3].active(), "traveling ink is exempt");
}

#[test]
fn without_zone_ink_the_static_blast_frame_bounds_pruning() {
    let mut n = crate::v1::SimState::spawn();
    n.paths[SHIP_SLOT] = InkPath::EMPTY; // isolate: the hull itself is zone material now
    n.paths[0] = body_at(&mut n.nodes, &mut n.free, Vector2::new(600.0, 500.0)); // well inside the frame
    n.paths[1] = body_at(
        &mut n.nodes,
        &mut n.free,
        Vector2::new(BLAST_RIGHT + 300.0, 500.0),
    ); // past the right edge
    assert!(
        ink_blast_zone(&n.paths, &n.nodes).is_none(),
        "no zone material down"
    );
    prune_outside(&mut n);
    assert!(n.paths[0].active());
    assert!(!n.paths[1].active());
}

#[test]
fn empty_and_baked_paths_are_not_bodies() {
    let p = InkPath::EMPTY;
    assert_eq!(p.mass, 0.0);
    assert!(!p.traveling());
    let mut moving = p;
    moving.vel = Vector2::new(5.0, 0.0);
    assert!(
        !moving.traveling(),
        "mass 0 never travels even with vel set"
    );
}

#[test]
fn tetris_material_never_expires() {
    let t = crate::v1::Tune::default();
    assert!(
        t.strokes.get(StrokeRegistry::TETRIS_ROW) == StrokeProps::TETRIS,
        "registry row 1"
    );
    let mut n = crate::v1::SimState::spawn();
    let mut perm = body_at(&mut n.nodes, &mut n.free, Vector2::new(600.0, 500.0));
    perm.props = StrokeProps::TETRIS;
    n.paths[0] = perm;
    // control: a TIMED stroke (the default pen is now also permanent, so the control sets an
    // explicit positive stroke_life to prove the decay loop still runs for finite material).
    let mut timed = body_at(&mut n.nodes, &mut n.free, Vector2::new(500.0, 500.0));
    timed.props = StrokeProps {
        stroke_life: 240,
        ..StrokeProps::PEN
    };
    n.paths[1] = timed;
    n.tick = 240 + 100; // well past the timed control's life
    let inputs = crate::v1::InputFrame::default();
    update_paths(&mut n, &[&inputs, &inputs], &t);
    assert!(n.paths[0].active(), "stroke_life < 0 = never expires");
    assert!(!n.paths[1].active(), "the timed control decayed");
}

#[test]
fn weak_strike_shakes_without_unlocking_and_strong_strike_launches() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut ink = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 500.0));
    // weak: the laser bolt's near-flat chip on a fresh stroke (center contact: no torque)
    resolve_hit_ink(&t.laser.hit, t.laser.hit.damage, 1.0, ink.pos, &mut ink, &t);
    assert_eq!(
        ink.vel,
        Vector2::ZERO,
        "below ink_launch_speed: still locked"
    );
    assert!(ink.shake > 0, "shakes as its hitstun");
    assert!(ink.percent > 0.0, "chip damage builds hp");
    // strong: the item-throw hit launches; an off-center contact spins it (the soccer ball)
    let percent_before = ink.percent;
    let corner = ink.world_pt(0, &sc.nodes);
    resolve_hit_ink(
        &t.throw_item.hit,
        t.throw_item.hit.damage,
        1.0,
        corner,
        &mut ink,
        &t,
    );
    assert!(ink.traveling(), "a real hit un-locks the body");
    assert!(
        ink.vel.x > 0.0 && ink.vel.y < 0.0,
        "launched up-and-out toward facing"
    );
    assert!(ink.omega != 0.0, "an off-center hit torques the body");
    assert!(ink.percent > percent_before);
}

#[test]
fn strikes_never_touch_baked_or_still_authored_ink() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut baked = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 500.0));
    baked.mass = 0.0; // baked stage sentinel
    resolve_hit_ink(&t.throw_item.hit, 9.0, 1.0, baked.pos, &mut baked, &t);
    assert_eq!(baked.vel, Vector2::ZERO);
    assert_eq!(baked.percent, 0.0);

    let mut drawing = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 500.0));
    drawing.drawing = true;
    resolve_hit_ink(&t.throw_item.hit, 9.0, 1.0, drawing.pos, &mut drawing, &t);
    assert_eq!(drawing.vel, Vector2::ZERO);
    assert_eq!(drawing.percent, 0.0);
}

#[test]
fn simplify_drops_collinear_points_and_keeps_bends() {
    // a flat run with redundant midpoints, then a sharp bend up
    let pts = [
        Vector2::new(0.0, 0.0),
        Vector2::new(50.0, 0.1), // ~collinear: dropped
        Vector2::new(100.0, 0.0),
        Vector2::new(150.0, 0.2), // ~collinear: dropped
        Vector2::new(200.0, 0.0),
        Vector2::new(200.0, -100.0), // the bend endpoint (kept: last)
    ];
    let out = simplify_polyline(&pts, 2.0);
    assert_eq!(out.first(), Some(&pts[0]), "first endpoint kept");
    assert_eq!(out.last(), Some(&pts[5]), "last endpoint kept");
    assert!(out.contains(&pts[4]), "the corner vertex survives");
    assert!(out.len() <= 3, "wobble collapses, got {:?}", out);
    // a real curve keeps enough points to stay within eps
    let arc: Vec<Vector2> = (0..=10)
        .map(|i| {
            let a = i as f32 * 0.31;
            Vector2::new(a.cos() * 100.0, a.sin() * 100.0)
        })
        .collect();
    let slim = simplify_polyline(&arc, 6.0); // 2-segment sagitta ~4.8px < eps: alternates drop
    assert!(
        slim.len() > 3 && slim.len() < arc.len(),
        "curve thins but keeps its bends: {}",
        slim.len()
    );
}

#[test]
fn rehydrated_stroke_matches_a_drawn_one() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let world = [
        Vector2::new(400.0, 500.0),
        Vector2::new(500.0, 500.0),
        Vector2::new(500.0, 400.0),
    ];
    let p = rehydrate_stroke(
        &world,
        StrokeRegistry::TETRIS_ROW,
        0,
        &mut sc.nodes,
        &mut sc.free,
        &t,
    );
    assert!(p.active() && !p.drawing);
    assert!(p.mass > 0.0, "a body on arrival");
    assert!(
        p.props == StrokeProps::TETRIS,
        "material off the registry row"
    );
    for (i, w) in world.iter().enumerate() {
        assert!(
            (p.world_pt(i, &sc.nodes) - *w).length() < 1e-3,
            "world geometry preserved"
        );
    }
    assert!(
        p.seg_class(0, &sc.nodes) == SegClass::Floor
            || p.seg_class(0, &sc.nodes) == SegClass::Ledge,
        "classified on load"
    );
}

#[test]
fn tetromino_is_a_closed_traveling_body() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    for shape in 0..TETROMINO_SHAPES {
        let p = tetromino_path(
            shape,
            Vector2::new(600.0, 300.0),
            Vector2::new(5.0, -8.0),
            StrokeProps::TETRIS,
            0,
            7,
            &mut sc.nodes,
            &mut sc.free,
        );
        let n = p.len as usize;
        assert!(p.traveling(), "shape {shape}: traveling from birth");
        assert!(p.mass > 0.0, "shape {shape}: a real body");
        assert!(
            (p.world_pt(0, &sc.nodes) - p.world_pt(n - 1, &sc.nodes)).length() < 1e-3,
            "shape {shape}: outline closes on itself"
        );
        assert!(
            (0..n - 1).any(|s| p.seg_class(s, &sc.nodes) == SegClass::Floor),
            "shape {shape}: has a standable top"
        );
        // closed loop + TETRIS ledge_curve: no seam lips, no corner lips — all-Floor tops
        // render blue, right-angle corners are never grabbable yellow.
        assert!(
            (0..n - 1).all(|s| p.seg_class(s, &sc.nodes) != SegClass::Ledge),
            "shape {shape}: a closed piece has no grabbable Ledge"
        );
    }
    // the O piece is character-sized: 2 cells = 100px across
    let o = tetromino_path(
        1,
        Vector2::new(600.0, 300.0),
        Vector2::ZERO,
        StrokeProps::TETRIS,
        0,
        0,
        &mut sc.nodes,
        &mut sc.free,
    );
    let xs: Vec<f32> = (0..o.len as usize)
        .map(|i| o.world_pt(i, &sc.nodes).x)
        .collect();
    let w =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!(
        (w - 2.0 * TETRIS_CELL).abs() < 1e-3,
        "O width = 2 cells, got {w}"
    );
    let _ = t;
}

#[test]
fn lobbed_piece_stacks_on_still_ink() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut paths = [InkPath::EMPTY; MAX_DRAWN];
    // a settled WIDE bar hovering above the floor (wider than the piece: settle tests nodes,
    // so a pad narrower than the corner spacing would let corners straddle it). Placed at
    // x=300 to stay clear of the stage's soft platforms.
    let mut pad = InkPath::EMPTY;
    pad.owner = 0;
    pad.drawing = true;
    pad.start = sc.free.alloc(MAX_PATH_PTS as u16).unwrap();
    pad.push(Vector2::new(150.0, 500.0), 0, &mut sc.nodes);
    pad.push(Vector2::new(450.0, 500.0), 1, &mut sc.nodes);
    finalize_path(&mut pad, &mut sc.nodes, &mut sc.free);
    paths[0] = pad;
    // an O piece dropped from above it
    paths[1] = tetromino_path(
        1,
        Vector2::new(300.0, 300.0),
        Vector2::new(0.0, 1.0),
        StrokeProps::TETRIS,
        0,
        0,
        &mut sc.nodes,
        &mut sc.free,
    );
    let mut frames = 0;
    while paths[1].traveling() && frames < 1200 {
        let snap = paths;
        integrate_ink(&mut paths[1], &snap, 1, &mut sc.nodes, &mut sc.free, &t);
        frames += 1;
    }
    assert!(!paths[1].traveling(), "piece locked within {frames} frames");
    let lowest = (0..paths[1].len as usize)
        .map(|i| paths[1].world_pt(i, &sc.nodes).y)
        .fold(f32::MIN, f32::max);
    assert!(
        (lowest - 500.0).abs() < 1e-2,
        "piece bottom rests ON the bar (y=500), got {lowest} — the stack"
    );
}

// --- generic solve + settle override (plans/body-unify.md step 2) --------------------------------
// The main stage floor is PLATFORMS[0]: y = 760 spanning x in [150, 1050], solid + invincible.
// x = 200 drops clear of every soft platform; x = 600 sits under the top-center one.

#[test]
fn restitution_zero_matches_the_old_hop_then_lock() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    // (a) the fast-impact frame must reproduce the old fast-branch velocity EXACTLY: normal
    // component dead-stopped, tangential scaled by ground friction, roll off the scaled
    // tangential. Byte-compatible for restitution 0 (bar `vel.y`'s sign-of-zero, documented).
    let mut fast = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 759.5));
    fast.props.bounce = 0.0;
    fast.vel = Vector2::new(10.0, 6.0); // approach > INK_SETTLE_SPEED after gravity
    integrate_ink(&mut fast, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
    let expected_tangential = 10.0_f32 * INK_BOUNCE_FRICTION;
    assert_eq!(
        fast.vel.x, expected_tangential,
        "tangential scaled by ground friction"
    );
    assert_eq!(
        fast.vel.y, 0.0,
        "normal component dead-stopped at restitution 0"
    );
    assert_eq!(
        fast.omega,
        expected_tangential * INK_ROLL_SPIN,
        "roll off the scaled tangential"
    );
    assert!(
        fast.traveling(),
        "a fast restitution-0 hop rolls on, does not lock this frame"
    );
    // (b) dropped onto the invincible stage floor, it bakes to an exact rest, like the old lock.
    let mut drop = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(200.0, 300.0));
    drop.props.bounce = 0.0;
    drop.vel = Vector2::new(0.0, 1.0);
    let mut frames = 0;
    while drop.traveling() && frames < 1200 {
        integrate_ink(&mut drop, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
        frames += 1;
    }
    assert!(
        !drop.traveling(),
        "restitution-0 stroke locks within {frames} frames"
    );
    assert_eq!(drop.vel, Vector2::ZERO, "baked to exact zero velocity");
    assert_eq!(drop.omega, 0.0);
    assert!(
        (0..drop.len as usize).any(|i| (drop.world_pt(i, &sc.nodes).y - 760.0).abs() < 1e-2),
        "settled resting on the invincible stage floor"
    );
}

#[test]
fn restitution_positive_bounces_off_the_baked_stage_then_bakes() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let empty = [InkPath::EMPTY; MAX_DRAWN];
    // (a) the one intended behavior change: a slow approach (<= INK_SETTLE_SPEED) the OLD code
    // LOCKED now reflects off the invincible stage -- the generic solve runs on a restitution>0
    // material before the settle override can bake. Reflected speed = -restitution * approach.
    let mut hit = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 759.5));
    hit.props.bounce = 0.4;
    hit.vel = Vector2::new(0.0, 2.0); // slow approach: the OLD code locked here
    integrate_ink(&mut hit, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
    let approach = 2.0_f32 + t.gravity * crate::v1::DT * crate::v1::DT * INK_FLOAT;
    assert!(
        hit.traveling(),
        "a bouncy stroke no longer locks on a slow stage hit"
    );
    assert!(hit.vel.y < 0.0, "it rebounds upward off the floor");
    assert!(
        (hit.vel.y - (-approach * 0.4)).abs() < 1e-5,
        "vel.y = -restitution * approach, got {} (approach {approach})",
        hit.vel.y
    );
    // (b) the override still bakes it once its rebound decays under INK_LOCK_REBOUND: it settles,
    // having bounced off the stage at least once first (the pre-refactor code locked immediately).
    let mut drop = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(200.0, 300.0));
    drop.props.bounce = 0.4;
    drop.vel = Vector2::new(0.0, 1.0);
    let (mut frames, mut bounces, mut prev_vy) = (0, 0, 0.0_f32);
    while drop.traveling() && frames < 2000 {
        integrate_ink(&mut drop, &empty, 0, &mut sc.nodes, &mut sc.free, &t);
        if drop.vel.y < 0.0 && prev_vy >= 0.0 {
            bounces += 1;
        }
        prev_vy = drop.vel.y;
        frames += 1;
    }
    assert!(
        !drop.traveling(),
        "the bouncy stroke bakes within {frames} frames"
    );
    assert_eq!(drop.vel, Vector2::ZERO, "settled at exact rest");
    assert!(
        bounces >= 1,
        "it bounced off the stage before baking (got {bounces})"
    );
    assert!(
        (0..drop.len as usize).any(|i| (drop.world_pt(i, &sc.nodes).y - 760.0).abs() < 1e-2),
        "resting on the stage floor after settling"
    );
}

#[test]
fn firing_the_tetris_gun_spawns_a_piece_not_a_projectile() {
    let t = crate::v1::Tune::default();
    let mut n = crate::v1::SimState::spawn();
    // hand P0 a tetris gun
    crate::v1::spawn_kind(
        &mut n,
        crate::v1::ItemKind::TetrisGun,
        ToolKind::TrailPen,
        StrokeRegistry::TETRIS_ROW,
        &t,
    );
    let k = n
        .items
        .iter()
        .position(|it| it.active() && it.mount < 0)
        .unwrap();
    n.items[k].owner = 0;
    n.fighters[0].holding = k as i8;
    let gas_before = n.items[k].gas;
    crate::v1::fire_gun(&mut n, 0, false, Vector2::ZERO, 0.0, &t);
    assert_eq!(n.items[k].gas, gas_before - 1.0, "one piece of ammo spent");
    let piece = n
        .paths
        .iter()
        .find(|p| p.active() && p.traveling())
        .expect("a path slot claimed");
    assert!(piece.traveling(), "the shot IS traveling ink");
    assert!(
        piece.props == StrokeProps::TETRIS,
        "piece wears the permanent material"
    );
    assert_eq!(piece.owner, 0, "attributed to the shooter");
    assert!(
        n.items
            .iter()
            .filter(|it| it.active() && it.mount < 0)
            .count()
            == 1,
        "no Item projectile spawned — the gun itself is the only field item"
    );
}

#[test]
fn full_board_evicts_the_oldest_stroke_and_a_dry_fire_keeps_ammo() {
    let t = crate::v1::Tune::default();
    let mut n = crate::v1::SimState::spawn();
    crate::v1::spawn_kind(
        &mut n,
        crate::v1::ItemKind::TetrisGun,
        ToolKind::TrailPen,
        StrokeRegistry::TETRIS_ROW,
        &t,
    );
    let k = n
        .items
        .iter()
        .position(|it| it.active() && it.mount < 0)
        .unwrap();
    n.items[k].owner = 0;
    n.fighters[0].holding = k as i8;
    // every path slot taken by a settled player stroke; slot 5 is the OLDEST (smallest born)
    for i in 0..MAX_DRAWN {
        let mut p = body_at(
            &mut n.nodes,
            &mut n.free,
            Vector2::new(200.0 + 40.0 * i as f32, 400.0),
        );
        p.stroke_born = if i == 5 { 1 } else { 100 + i as u64 };
        n.paths[i] = p;
    }
    let gas = n.items[k].gas;
    crate::v1::fire_gun(&mut n, 0, false, Vector2::ZERO, 0.0, &t);
    assert_eq!(n.items[k].gas, gas - 1.0, "the shot happened: ammo spent");
    assert!(
        n.paths[5].traveling(),
        "the oldest stroke's slot now holds the flying piece"
    );
    assert!(
        n.paths[5].props == StrokeProps::TETRIS,
        "and it wears the piece material"
    );

    // nothing evictable (all baked stage strokes): the shot never happens, ammo stays
    let mut n2 = crate::v1::SimState::spawn();
    crate::v1::spawn_kind(
        &mut n2,
        crate::v1::ItemKind::TetrisGun,
        ToolKind::TrailPen,
        StrokeRegistry::TETRIS_ROW,
        &t,
    );
    let k2 = n2
        .items
        .iter()
        .position(|it| it.active() && it.mount < 0)
        .unwrap();
    n2.items[k2].owner = 0;
    n2.fighters[0].holding = k2 as i8;
    for i in 0..MAX_DRAWN {
        let mut p = body_at(
            &mut n2.nodes,
            &mut n2.free,
            Vector2::new(200.0 + 40.0 * i as f32, 400.0),
        );
        p.owner = -1;
        n2.paths[i] = p;
    }
    let gas2 = n2.items[k2].gas;
    crate::v1::fire_gun(&mut n2, 0, false, Vector2::ZERO, 0.0, &t);
    assert_eq!(
        n2.items[k2].gas, gas2,
        "dry fire against an un-evictable board keeps the ammo"
    );
    assert!(n2.paths.iter().all(|p| !p.traveling()), "and lobs nothing");
}

#[test]
fn strike_ink_hits_by_contact_and_reports_it() {
    let t = crate::v1::Tune::default();
    let mut sc = Scratch::new();
    let mut paths = [InkPath::EMPTY; MAX_DRAWN];
    paths[0] = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 500.0)); // 60px bar at y=500
    let hb = t.throw_item.hit;
    // graze the bar's midpoint
    assert!(strike_ink(
        &mut paths,
        Vector2::new(600.0, 510.0),
        hb.r,
        &hb,
        hb.damage,
        1.0,
        &sc.nodes,
        &t
    ));
    assert!(paths[0].traveling());
    // far away: no contact
    let mut paths2 = [InkPath::EMPTY; MAX_DRAWN];
    paths2[0] = body_at(&mut sc.nodes, &mut sc.free, Vector2::new(600.0, 500.0));
    assert!(!strike_ink(
        &mut paths2,
        Vector2::new(100.0, 100.0),
        hb.r,
        &hb,
        hb.damage,
        1.0,
        &sc.nodes,
        &t
    ));
    assert_eq!(paths2[0].percent, 0.0);
}

// --- billiards (plans/body-bus.md step 6) ---------------------------------------------------------

/// Two O pieces side by side: `a` traveling right into a still `b`. Built into the SimState's own
/// pool so the handles and geometry stay coherent.
fn billiard_pair(speed: f32) -> (crate::v1::SimState, crate::v1::Tune) {
    let t = crate::v1::Tune::default();
    let mut s = crate::v1::SimState::spawn();
    let mut a = tetromino_path(
        1,
        Vector2::new(400.0, 300.0),
        Vector2::new(speed, 0.0),
        StrokeProps::TETRIS,
        0,
        1,
        &mut s.nodes,
        &mut s.free,
    );
    a.props.stroke_life = -1; // no decay mid-test
    let mut b = tetromino_path(
        1,
        Vector2::new(508.0, 300.0), // O is 100 wide: faces ~8px apart, inside contact reach
        Vector2::ZERO,
        StrokeProps::TETRIS,
        0,
        2,
        &mut s.nodes,
        &mut s.free,
    );
    b.props.stroke_life = -1;
    assert!(a.traveling() && !b.traveling());
    s.paths[0] = a;
    s.paths[1] = b;
    (s, t)
}

#[test]
fn hard_billiard_unlocks_the_still_piece_and_conserves_momentum() {
    let (mut s, t) = billiard_pair(12.0); // px/frame, way past ink_launch_speed after /DT
    let before = s.paths[0].vel * s.paths[0].mass + s.paths[1].vel * s.paths[1].mass;
    resolve_ink_billiard(&mut s, &t);
    assert!(
        s.paths[1].traveling(),
        "hard knock un-locks the still piece (vel = {:?})",
        s.paths[1].vel
    );
    assert!(s.paths[1].vel.x > 0.0, "it flies away from the impact");
    let after = s.paths[0].vel * s.paths[0].mass + s.paths[1].vel * s.paths[1].mass;
    // equal masses, spin clamped only on extremes: linear momentum carries within tolerance
    assert!(
        (before - after).length() < before.length() * 0.2,
        "momentum roughly conserved: {before:?} -> {after:?}"
    );
}

#[test]
fn weak_billiard_only_shakes_the_still_piece() {
    let (mut s, t) = billiard_pair(0.9); // barely moving: sub-threshold knock
    resolve_ink_billiard(&mut s, &t);
    assert!(
        !s.paths[1].traveling(),
        "a weak nudge must not un-lock the platform under someone"
    );
    assert!(s.paths[1].shake > 0, "it jiggles in place instead");
    assert!(
        s.paths[0].vel.x <= 0.9,
        "the traveler bounced off immovable mass (vel.x = {})",
        s.paths[0].vel.x
    );
}

#[test]
fn flying_piece_detonates_a_bomb() {
    let (mut s, t) = billiard_pair(12.0);
    s.paths[1] = InkPath::EMPTY; // just the traveler
    s.items[0] = crate::v1::Item {
        kind: crate::v1::ItemKind::Bomb,
        pos: Vector2::new(460.0, 300.0), // in the traveler's face
        vel: Vector2::ZERO,
        owner: 0,
        gas: 0.0,
        gas_max: 1.0,
        timer: 600,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
    crate::v1::item::ink_hits_items(&mut s, &t);
    assert!(!s.items[0].active(), "bomb pops on ink slam");
}

// ── StageSpec (plans/turnkey-extension.md S1): STAGE0-derived consts must equal the pinned
// legacy literals, so the extraction is provably behavior-preserving. The blast_left / ship
// numbers moved when the hull grew ~1.8x (plans/ac-ship-backlog.md item 3); re-derived here. ──

#[test]
fn stage0_floor_and_blast_consts_match_legacy_literals() {
    assert_eq!(GROUND_Y, 760.0);
    assert_eq!(STAGE_BOTTOM, 900.0);
    assert_eq!(FLOOR_LEFT, 150.0);
    assert_eq!(FLOOR_RIGHT, 1050.0);
    assert_eq!(BLAST_Y, 1600.0);
    assert_eq!(BLAST_TOP, -520.0);
    assert_eq!(BLAST_LEFT, -560.0); // pushed out to seat the bigger hull off-stage-left
    assert_eq!(BLAST_RIGHT, 1620.0);

    assert_eq!(STAGE0.ground_y, 760.0);
    assert_eq!(STAGE0.stage_bottom, 900.0);
    assert_eq!(STAGE0.floor_left, 150.0);
    assert_eq!(STAGE0.floor_right, 1050.0);
    assert_eq!(STAGE0.blast_y, 1600.0);
    assert_eq!(STAGE0.blast_top, -520.0);
    assert_eq!(STAGE0.blast_left, -560.0);
    assert_eq!(STAGE0.blast_right, 1620.0);
}

#[test]
fn stage0_platform_table_matches_legacy_literals() {
    let expect = [
        (150.0, 1050.0, 760.0, true), // solid main stage
        (280.0, 540.0, 575.0, false), // left soft platform
        (660.0, 920.0, 575.0, false), // right soft platform
        (470.0, 730.0, 410.0, false), // top-center soft platform
    ];
    assert_eq!(PLATFORMS.len(), expect.len());
    for (p, (left, right, y, solid)) in PLATFORMS.iter().zip(expect) {
        assert_eq!(p.left, left);
        assert_eq!(p.right, right);
        assert_eq!(p.y, y);
        assert_eq!(p.solid, solid);
    }
    // PLATFORMS is a direct re-derivation of STAGE0.platforms, not a second literal table.
    for (a, b) in PLATFORMS.iter().zip(STAGE0.platforms.iter()) {
        assert_eq!(a.left, b.left);
        assert_eq!(a.right, b.right);
        assert_eq!(a.y, b.y);
        assert_eq!(a.solid, b.solid);
    }
}

#[test]
fn stage0_fixture_slots_and_geometry_match_pinned_literals() {
    assert_eq!(PILLAR_SLOT, MAX_DRAWN - 2);
    assert_eq!(MOVER_SLOT, MAX_DRAWN - 3);
    assert_eq!(STAGE0.pillar_slot, PILLAR_SLOT);
    assert_eq!(STAGE0.mover_slot, MOVER_SLOT);

    assert_eq!(PILLAR_X, 980.0); // right of the right soft platform (920), inside the lip (1050)
    assert_eq!(PILLAR_TOP, 560.0);
    assert_eq!(PILLAR_BOT, 685.0); // stops short of a grounded ECB center (690): floor traffic passes
    assert_eq!(MOVER_W, 180.0);
    assert_eq!(MOVER_AMP, 220.0);
    assert_eq!(MOVER_PERIOD, 360);
    // upper right: the sweep (730..1350) clears every spawn column, and 220 sits above the
    // y=250 spawn drop-in height — fighters falling in never cross the mover's face.
    assert_eq!(MOVER_HOME, Vector2::new(1040.0, 220.0));

    // the pillar's one segment classifies as a Wall (plumb beats every wall_tol) and both
    // fixtures carry the baked-stage sentinel (mass 0 = immovable/unstrikeable/prune-exempt).
    let mut sc = Scratch::new();
    let pillar = bake_pillar(&mut sc.nodes, &mut sc.free);
    assert_eq!(pillar.seg_class(0, &sc.nodes), SegClass::Wall);
    assert_eq!(pillar.mass, 0.0);
    assert!(pillar.owner < 0);
    let mover = bake_mover(&mut sc.nodes, &mut sc.free);
    assert!(matches!(
        mover.seg_class(0, &sc.nodes),
        SegClass::Floor | SegClass::Ledge
    ));
    assert_eq!(mover.mass, 0.0);
    assert!(mover.owner < 0);
    assert_eq!(mover.pos, mover_pos(0));
}

#[test]
fn stage0_ship_geometry_matches_legacy_literals() {
    assert_eq!(SHIP_SLOT, MAX_DRAWN - 1);
    assert_eq!(SHIP_R, 306.0); // ~1.8x the original 170px hull
    assert_eq!(SHIP_HOME, Vector2::new(-190.0, 520.0));
    assert_eq!(SHIP_SEGS, 20);
    assert_eq!(SHIP_RIM.len(), 20);
    assert_eq!(SHIP_RIM[0], Vector2::new(1.0, 0.0));
    assert_eq!(SHIP_RIM[5], Vector2::new(0.0, 1.0));
    assert_eq!(SHIP_RIM[10], Vector2::new(-1.0, 0.0));
    assert_eq!(SHIP_RIM[15], Vector2::new(0.0, -1.0));

    assert_eq!(STAGE0.ship_slot, SHIP_SLOT);
    assert_eq!(STAGE0.ship_r, SHIP_R);
    assert_eq!(STAGE0.ship_home, SHIP_HOME);
    assert_eq!(STAGE0.ship_segs, SHIP_SEGS);
    assert_eq!(STAGE0.ship_rim, SHIP_RIM);
}
