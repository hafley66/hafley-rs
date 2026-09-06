//! Baked "invincible ink" test fixtures: a vertical wall pillar (walljump-on-ink) and the
//! first real Randall — a deterministic moving platform. Both are stage-class strokes cut
//! from the ship's cloth (`bake_ship`): PEN material at density 0, owner -1, so strikes read
//! `Guard::Invuln`, `prune_outside` exempts them, and the drawn-ink free-slot scan skips them
//! by occupancy. The mover's position is a PURE function of `SimState.tick` (a piecewise-
//! linear triangle wave — no runtime trig, bit-identical across platforms), recomputed every
//! `step`, so rollback can never drift it.

use crate::v1::arena::{FreeSpans, InkNode};
use crate::v1::stage::{InkPath, STAGE0, StrokeProps, bake_span};
use crate::v1::{SimState, Vector2};

/// `SimState.paths[PILLAR_SLOT]` is the wall pillar. Reserved by occupancy, like the ship.
pub const PILLAR_SLOT: usize = STAGE0.pillar_slot;
/// `SimState.paths[MOVER_SLOT]` is the moving platform. Reserved by occupancy, like the ship.
pub const MOVER_SLOT: usize = STAGE0.mover_slot;

/// Pillar x: over the main floor, right of the right soft platform (ends at 920), left of the
/// stage lip (1050) — clear of every platform, reachable with a hop.
pub const PILLAR_X: f32 = 980.0;
/// Pillar top y.
pub const PILLAR_TOP: f32 = 560.0;
/// Pillar bottom y: 75px ABOVE the floor, so the wall band stops short of a grounded ECB
/// center (GROUND_Y - ECB_HALF_H = 690) and ground traffic passes beneath. On-floor placement
/// is impossible on the right side: a ledge roll off the right lip travels ~400px inward
/// (techroll_speed x techroll_frames) and a floor-standing wall anywhere in that lane blocks it.
pub const PILLAR_BOT: f32 = 685.0;

/// Mover platform width (px): a flat 2-point floor seg.
pub const MOVER_W: f32 = 180.0;
/// Mover sweep half-range (px): the platform center travels HOME.x ± this.
pub const MOVER_AMP: f32 = 220.0;
/// Frames per full left-right-left cycle of the triangle wave.
pub const MOVER_PERIOD: u64 = 360;
/// Mover center at phase midpoint: a sky platform on the UPPER RIGHT, above the pillar. Its
/// whole sweep (home.x ± (AMP + W/2) = 730..1350) stays right of every spawn/drop-in column
/// (480/720) and above the y=250 spawn height, so fighters falling in never cross its face at
/// any phase — the mover is opt-in terrain, reached by jumping (or walljumping the pillar).
pub const MOVER_HOME: Vector2 = Vector2::new(1040.0, 220.0);

/// A vertical 2-point stroke — one Wall segment (classify: dx = 0 is plumb, past every
/// `wall_tol`), for walljump testing on purple ink. Same discipline as the hull: PEN,
/// density 0 (mass 0 = immovable, unstrikeable, zone/prune-exempt).
// parity(v1-stage-pillar-geometry): bake_pillar places the fixed vertical wall fixture from the literal stage coordinates
pub fn bake_pillar(nodes: &mut [InkNode], free: &mut FreeSpans) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.props = StrokeProps::PEN;
    p.props.density = 0.0;
    let half_h = (PILLAR_BOT - PILLAR_TOP) * 0.5;
    let local = [Vector2::new(0.0, -half_h), Vector2::new(0.0, half_h)];
    bake_span(
        &mut p,
        &local,
        Vector2::new(PILLAR_X, PILLAR_TOP + half_h),
        nodes,
        free,
    );
    p
}

/// A flat 2-point floor stroke parked at `mover_pos(0)`. Soft like every PEN floor (land from
/// above, drop through with down). `drive_mover` re-derives its `pos`/`vel` from the tick each
/// frame; the geometry itself never changes, so `classify` runs once here and never again.
// parity(v1-stage-mover): bake_mover and mover_pos define the soft moving platform and its triangle-wave travel
pub fn bake_mover(nodes: &mut [InkNode], free: &mut FreeSpans) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.props = StrokeProps::PEN;
    p.props.density = 0.0;
    let local = [
        Vector2::new(-MOVER_W * 0.5, 0.0),
        Vector2::new(MOVER_W * 0.5, 0.0),
    ];
    bake_span(&mut p, &local, mover_pos(0), nodes, free);
    p
}

/// The mover's center at a tick: a horizontal triangle wave about `MOVER_HOME`, amplitude
/// `MOVER_AMP`, period `MOVER_PERIOD`. Pure and integer-derived (`tick % period` before any
/// float touches it) — `mover_pos(t) == mover_pos(t + MOVER_PERIOD)` exactly, so rollback
/// re-simulation always reproduces the same position. Starts at the LEFT extreme, sweeps
/// right for a half-period, then back.
pub fn mover_pos(tick: u64) -> Vector2 {
    let ph = (tick % MOVER_PERIOD) as f32;
    let half = (MOVER_PERIOD / 2) as f32;
    let dx = if ph < half {
        -MOVER_AMP + 2.0 * MOVER_AMP * ph / half
    } else {
        MOVER_AMP - 2.0 * MOVER_AMP * (ph - half) / half
    };
    Vector2::new(MOVER_HOME.x + dx, MOVER_HOME.y)
}

/// This frame's displacement (px/FRAME — ink's native velocity unit): the exact difference
/// `pos(tick) - pos(tick - 1)`, so the ride carry and the wave never disagree, turnaround
/// frames included. ±2·AMP/half on straight runs.
pub fn mover_step(tick: u64) -> Vector2 {
    mover_pos(tick) - mover_pos(tick.wrapping_sub(1))
}

/// Re-derive the mover's body from the tick. Called at the top of `step`, BEFORE the frame's
/// ink snapshot / surf soup, so every sweep and pin this frame sees the current-tick platform.
/// Writing pos from tick (never integrating) is what makes the mover rollback-proof. `vel`
/// rides into the soup via the `Randall` emit (px/frame there, converted to px/s) for the
/// landing inherit, and the grounded-ink carry reads it directly.
pub(crate) fn drive_mover(n: &mut SimState) {
    let p = &mut n.paths[MOVER_SLOT];
    if !p.active() || p.owner >= 0 {
        return; // slot isn't the baked mover (test states that never spawned it)
    }
    p.pos = mover_pos(n.tick);
    p.vel = mover_step(n.tick);
}
