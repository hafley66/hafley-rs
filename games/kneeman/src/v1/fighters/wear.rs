//! Per-badge wear timer (queue-2026-07-03 item 6): the DEFAULT de-spawn condition for a badge --
//! N frames after attach, the bit auto-clears. Generalizes the AC fuel-meter pattern
//! (`ac::AC_GAS_FRAMES` / `Fighter.ac_gas`) to every badge EXCEPT `AcCore`, which keeps its own
//! meter (folding mech thrust fuel into this table risked that coupling; see `ac.rs`). One
//! fixed-size `Copy` field on `Fighter` (`badge_gas`), indexed by the badge's bit position, so a
//! future badge opts into the default timeout for free -- no new field, no new match arm.

use crate::v1::{Badge, Fighter, N_BADGE_BITS};

/// Default wear window for any badge that doesn't own a bespoke meter: 600 frames (10s @ 60Hz),
/// matching `ac::AC_GAS_FRAMES`'s cadence.
pub(crate) const DEFAULT_BADGE_WEAR_FRAMES: i64 = 600;

/// Bit position of a badge within the `Fighter.badges` mask -- the index into `badge_gas`.
fn bit_index(b: Badge) -> usize {
    (b as u8).trailing_zeros() as usize
}

/// Frames of wear a badge gets on attach, or `None` to opt out of the generic table (the badge
/// manages its own de-spawn condition elsewhere -- today only `AcCore`, via `ac_gas`). New badges
/// fall through the wildcard and get the default timeout automatically: this IS the default.
fn wear_frames(b: Badge) -> Option<i64> {
    match b {
        Badge::AcCore => None,
        _ => Some(DEFAULT_BADGE_WEAR_FRAMES),
    }
}

/// Arm the wear timer for a freshly-attached badge. Called from `acts::attach_badge`; a no-op
/// for a badge that opts out of the table.
pub(crate) fn arm(f: &mut Fighter, badge: Badge) {
    if let Some(frames) = wear_frames(badge) {
        f.badge_gas[bit_index(badge)] = frames;
    }
}

/// Tick every armed slot down one frame (paused during hitlag, same as `ac_gas`); a slot that
/// reaches 0 clears its badge bit. Called once per active fighter per frame from
/// `fighters::tick_badge_meters`.
pub(crate) fn tick(f: &mut Fighter) {
    if f.hitlag != 0 {
        return;
    }
    for bit in 0..N_BADGE_BITS {
        if f.badge_gas[bit] > 0 {
            f.badge_gas[bit] -= 1;
            if f.badge_gas[bit] == 0 {
                f.badges &= !(1 << bit);
            }
        }
    }
}

/// Wear frames remaining for one badge's slot (test convenience -- keeps assertions off the raw
/// bit index).
#[cfg(test)]
pub(crate) fn gas_for(f: &Fighter, badge: Badge) -> i64 {
    f.badge_gas[bit_index(badge)]
}
