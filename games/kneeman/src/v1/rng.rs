//! The sim's one deterministic RNG step, and its `Zoom` wiring onto `SimState.rng`.
//! `Zoom`'s first real consumer (core-rx-refactor.md row 10): proves the combinator
//! composes over the actual rollback `State`, not a toy. Split out of `item.rs` (the
//! LCG's only caller) so the Zoom/Lens ceremony doesn't grow that file past its R5
//! line-budget ratchet.

use crate::v1::SimState;
use crate::v1::slice::{Lens, Never, Slice, Zoom};

/// Deterministic LCG step (same constants as the SyncTest's generator). Advances `state` and
/// returns the high bits. Pure + integer, so both peers stay in lockstep.
fn next_rng(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

/// Static lens onto `SimState.rng`: the other half of the same `Lens` shape the generated
/// `tune_paths::lens::*` ZSTs use (kit S4's dl-codegen track), just hand-written here since
/// `SimState` fields aren't a `dl`-elected struct.
#[derive(Copy, Clone)]
struct RngLens;
impl Lens<SimState, u64> for RngLens {
    fn get(outer: &SimState) -> &u64 {
        &outer.rng
    }
    fn get_mut(outer: &mut SimState) -> &mut u64 {
        &mut outer.rng
    }
}

/// `next_rng` as a `Slice`: `State` is the seed alone, so this reducer knows nothing about
/// `SimState` -- `Zoom<RngLens, NextRng, SimState>` is what re-addresses it onto the real
/// `rng` field.
struct NextRng;
impl Slice for NextRng {
    type Context<'a> = ();
    type State = u64;
    type Event = ();
    type Output = u64;
    type Effect = Never;
    #[inline(always)]
    fn reduce(st: &mut u64, _ev: (), _cx: (), _fx: &mut impl FnMut(Never)) -> u64 {
        next_rng(st)
    }
}

/// Named alias so the size assert's own `<..>` has no nested generic (a bare
/// `Zoom<RngLens, NextRng, SimState>` reads fine to rustc, but the `.dl/slice-zst.dl` rail's
/// `size_of::<[^>]*>` scan stops at the first `>` and would miss a directly-nested one).
type ZoomRng = Zoom<RngLens, NextRng, SimState>;
const _: () = assert!(core::mem::size_of::<ZoomRng>() == 0);

/// Draw the next deterministic value, composed through `Zoom` over `SimState` rather than a
/// bare `next_rng(&mut n.rng)` call -- every item.rs call site goes through this so the
/// composition isn't a parallel unused path next to the free function.
pub(crate) fn roll_rng(n: &mut SimState) -> u64 {
    <ZoomRng as Slice>::reduce(n, (), (), &mut |never: Never| match never {})
}
