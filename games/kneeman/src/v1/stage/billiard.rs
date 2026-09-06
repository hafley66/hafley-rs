//! Ink-vs-ink billiard resolve: the traveling↔ink impulse solver and its writeback helpers.
//! Split out of `stage/mod.rs` (2026-07-07) to make headroom under the R5 file-line ratchet
//! (`.dl/lint-file-budget.dl`); pre-stages this file as the home for a future broadphase pass
//! too. Pure code motion — no behavior change, callers unaffected (re-exported from `stage`).

use crate::v1::arena::InkNode;
use crate::v1::body::{BodyBits, collide, contact};
use crate::v1::stage::{INK_MAX_SPIN, INK_MU, InkPath, MAX_DRAWN, classify};
use crate::v1::{DT, SimState, Tune, Vector2};

/// Frames of in-place jiggle when a billiard knock is too weak to un-lock a still piece.
const INK_NUDGE_SHAKE: i64 = 4;

/// Traveling↔ink billiards (plans/body-bus.md step 6): the plain all-pairs double loop,
/// ascending `(i, j)`. The narrow phase (`resolve_pair` → `contact::ink_vs_ink`) is the real
/// gate — its bound-circle cull rejects non-touching pairs cheaply, and MAX_DRAWN caps the
/// pair count, so no broadphase structure is needed.
///
/// CANONICAL ORDER IS LOAD-BEARING: `resolve_pair` mutates `n.paths[i]`/`n.paths[j]` (vel/omega/
/// shake), and each pair re-reads both bodies fresh at the top, so a pair resolved earlier
/// changes the velocity a later pair reads. Ascending `(i, j)` is the canonical order every
/// peer replays.
// parity(v1-ink-billiard-response): traveling strokes exchange linear and angular impulse in stable pair order, with valves filtering before solve and weak impacts leaving locked ink in place
pub(crate) fn resolve_ink_billiard(n: &mut SimState, t: &Tune) {
    for i in 0..MAX_DRAWN {
        for j in (i + 1)..MAX_DRAWN {
            resolve_pair(n, t, i, j);
        }
    }
}

/// One billiard pair's narrow-phase test + impulse resolve, factored out per pair. Re-reads `n.paths[i]`/`n.paths[j]`
/// fresh (velocities may have moved since the tree was built) and mutates them in place. Full
/// rigid impulse by mass and inertia, restitution `max(bounce_a, bounce_b)` (Box2D's pairing),
/// friction-driven spin transfer. A STILL piece runs the launch-threshold policy: compute the
/// exchange as if it were free — if the speed it would take clears `ink_launch_speed` it un-locks
/// and the chain propagates; below that it jiggles in place and the traveler bounces off it as
/// immovable mass. Baked stage strokes (mass 0) are always immovable.
pub(crate) fn resolve_pair(n: &mut SimState, t: &Tune, i: usize, j: usize) {
    let (a, b) = (n.paths[i], n.paths[j]);
    let body = |p: &InkPath| p.active() && !p.drawing;
    if !body(&a) || !body(&b) || !(a.traveling() || b.traveling()) {
        return;
    }
    let Some(hit) = contact::ink_vs_ink(&a, &b, &n.nodes) else {
        return;
    };
    if contact::ink_pair_gate_admits(&a.props, &b.props, &hit) {
        return; // one-way filter (plans/body-unify.md step 5): an open gate drops it
    }
    let e = a.props.bounce.max(b.props.bounce);
    // pass 1, as-if-free: what would each side take if both were live bodies?
    let (mut fa, mut fb) = (a.body_bits(&n.nodes), b.body_bits(&n.nodes));
    collide(&mut fa, &mut fb, hit.point, hit.normal, e, INK_MU);
    // the un-lock test, per still side: would-be speed in px/s vs the threshold
    let unlocks = |p: &InkPath, would: &BodyBits| {
        p.traveling() || p.mass <= 0.0 || would.vel.length() / DT >= t.ink_launch_speed
    };
    let a_free = unlocks(&a, &fa);
    let b_free = unlocks(&b, &fb);
    if a_free && b_free {
        write_billiard(&mut n.paths[i], &fa);
        write_billiard(&mut n.paths[j], &fb);
    } else {
        // pass 2: the locked side is immovable; the traveler bounces off it.
        let (mut ga, mut gb) = (a.body_bits(&n.nodes), b.body_bits(&n.nodes));
        let pin = |g: &mut BodyBits| {
            g.inv_mass = 0.0;
            g.inv_inertia = 0.0;
            g.vel = Vector2::ZERO;
            g.omega = 0.0;
        };
        if !a_free {
            pin(&mut ga);
        } else {
            pin(&mut gb);
        }
        collide(&mut ga, &mut gb, hit.point, hit.normal, e, INK_MU);
        if a_free {
            write_billiard(&mut n.paths[i], &ga);
            n.paths[j].shake = n.paths[j].shake.max(INK_NUDGE_SHAKE);
        } else {
            write_billiard(&mut n.paths[j], &gb);
            n.paths[i].shake = n.paths[i].shake.max(INK_NUDGE_SHAKE);
        }
    }
}

/// Billiard writeback: vel/omega only (pos never teleports), spin clamped to the same
/// cap strikes use. A still piece that took a clearing impulse is now traveling — the
/// un-lock — and `shake` clears because the travel IS the reaction.
pub(crate) fn write_billiard(p: &mut InkPath, bits: &BodyBits) {
    if p.mass <= 0.0 {
        return; // baked stage: immovable no matter what the solver says
    }
    p.vel = bits.vel;
    p.omega = bits.omega.clamp(-INK_MAX_SPIN, INK_MAX_SPIN);
    if p.traveling() {
        p.shake = 0;
    }
}

/// Fold a flight rotation into the body's geometry: rotate every local offset by `rot`, zero it,
/// and re-classify (segment slopes changed with the orientation). After this the path is a plain
/// axis-fixed stroke again — the only place `rot` is ever consumed.
pub(crate) fn bake_rotation(p: &mut InkPath, nodes: &mut [InkNode]) {
    if p.rot == 0.0 {
        return;
    }
    let (s, c) = p.rot.sin_cos();
    let start = p.start as usize;
    for offset in 0..p.len as usize {
        let v = nodes[start + offset].pt;
        nodes[start + offset].pt = Vector2::new(v.x * c - v.y * s, v.x * s + v.y * c);
    }
    p.rot = 0.0;
    classify(p, nodes);
}
