//! The `anchor` primitive (plans/architecture-debt.md #1; plans/move-language.md "the third
//! noun"): entity-to-entity transform slaving -- one body (the rider) rides another's (the
//! host's) coordinate system until release. Grab-hold (`moves/throw.rs`), station-mount
//! (`station.rs`), ledge-hang (`physics::ledge_ride`), and ship-rider
//! (`step::repin_ink_riders`) each hand-roll this same shape today, each independently
//! scrubbing stale ground refs and fighting the "grounded" invariant. This module extracts
//! the PIN math (position slaving + the stale-host scrub) as one pure function so it stops
//! being reimplemented per site.
//!
//! Scope so far: `step::repin_ink_riders` (ship-rider) and `station::stationed_step`
//! (station-mount) are migrated. Grab-hold (`moves/throw.rs`) and ledge-hang
//! (`physics::ledge_ride`) are still follow-up work (architecture-debt.md #1) and are
//! untouched here. Station-mount has no incremental carry to correct -- it degenerates to a
//! plain re-read-and-overwrite each tick (`pos_snapshot` is the rider's own current position,
//! `carry_applied` is zero), which the general delta-correction formula collapses to exactly;
//! see `station::stationed_step`'s doc for the algebra. The `release` predicate (when a rider
//! stops being slaved) stays per-site by design -- release conditions differ enough across
//! grab/station/ledge/ship that unifying them is not this primitive's job; only the pin
//! geometry unifies.

use crate::v1::Vector2;

/// The host body's translation this tick, as seen from the rider's point of view: where the
/// host was at the frame-start snapshot the rider's carry was computed against, and where it
/// actually ended up after this tick's own integration (gravity solve, billiard impulse, wall
/// bounce, ...). `active`/`traveling` are the stale-ref scrub: a host that died this tick
/// (its stroke pruned/expired) or was never actually "traveling" under the snapshot (a
/// kinematic fixture driven directly from ground truth, so the rider's carry was never stale
/// against it) has nothing true to correct toward.
pub struct AnchorHost {
    pub pos_snapshot: Vector2,
    pub pos_now: Vector2,
    pub active: bool,
    pub traveling: bool,
}

/// Re-pin a rider slaved to `host`. `carry_applied` is how much of the host's motion the
/// caller already baked into `rider_pos` earlier this tick, per axis -- a caller whose ride
/// mechanic only ever applies the x carry (y is re-derived some other way) passes zero for
/// y, rather than this function branching per component. One vector op computes the whole
/// correction: the host's REAL end-of-tick delta minus the carry already applied. Returns
/// `rider_pos` unchanged when the stale-ref scrub (`host.active`/`host.traveling`) finds
/// nothing true to correct toward.
pub fn anchor_pin(rider_pos: Vector2, host: AnchorHost, carry_applied: Vector2) -> Vector2 {
    if !host.active || !host.traveling {
        return rider_pos;
    }
    let real_delta = host.pos_now - host.pos_snapshot;
    rider_pos + (real_delta - carry_applied)
}
