//! The character as a pluggable `Slice` (plans/core-rx-refactor.md section 3). One fighter, one
//! input frame, emits one `Act`. The body is `reduce_next_state` (za_warudo.rs) verbatim; the free
//! fn stays put, only its call site moves behind this ZST. This is the seam a per-character folder
//! (`fighters/`) grows from later; for now the FSM logic stays in za_warudo.rs.

pub(crate) mod wear; // per-badge wear timer (relocated from items/, queue-2026-07-03 item 6 + core-rx-refactor row 8): fighter-owned state, so it lives with the fighter, not the item

use crate::v1::slice::{Never, Slice};
use crate::v1::za_warudo::reduce_next_state;
use crate::v1::{
    Act, Badge, Fighter, InkNode, InkPath, InputFrame, Item, MAX_DRAWN, MAX_ITEMS, MAX_PLAYERS,
    SimState, Tune, Vector2, ZoneRect,
};

/// One fighter's frame-start view of everything cross-entity: the item array, the ink snapshot,
/// the hurtbox foes (self slot already `None`), this fighter's tune row, and the blast rect. Built
/// per slot by the `Fsm` phase; every reference is a frame-start snapshot, so slot iteration order
/// cannot leak into an FSM result. `foes` is held BY VALUE (not `&'a [..]` as first sketched): the
/// self-slot-`None` is a per-slot edit, so each slot owns its own small Copy array.
#[derive(Copy, Clone)]
pub struct FighterCx<'a> {
    pub items: &'a [Item; MAX_ITEMS],
    pub paths: &'a [InkPath; MAX_DRAWN],
    pub nodes: &'a [InkNode], // the shared ink-node arena the paths handle into
    pub foes: [Option<(Vector2, f32)>; MAX_PLAYERS], // self slot already None
    pub tune: &'a Tune,       // per-port row from tunes
    pub zone: Option<ZoneRect>,
}

/// The fighter FSM as a `Slice`. `reduce_next_state` stays a free fn; this only relocates the call.
pub struct FighterSlice;
impl Slice for FighterSlice {
    type Context<'a> = FighterCx<'a>;
    type State = Fighter;
    type Event = InputFrame;
    type Output = Act;
    type Effect = Never;
    #[inline(always)]
    fn reduce(
        st: &mut Fighter,
        ev: InputFrame,
        cx: FighterCx<'_>,
        _fx: &mut impl FnMut(Never),
    ) -> Act {
        reduce_next_state(
            st, cx.items, cx.paths, cx.nodes, &cx.foes, &ev, cx.tune, cx.zone,
        )
    }
}
const _: () = assert!(core::mem::size_of::<FighterSlice>() == 0);

/// Per-badge meter maintenance: the AC fuel meter drains its `AcCore` bit at 0, and every other
/// armed badge wears down via `wear::tick`. Reads/writes only the fighter itself, so it belongs to
/// the fighter, not to items -- this is the Phase C1 move OUT of `update_items` (item.rs).
///
/// Runs at the START of the Items phase (before the item loop), the exact pipeline point it held
/// inside `update_items`. It is NOT folded into the `Fsm`/`reduce_next_state` scan: the `ac_gas`
/// and `wear::tick` hitlag gate must see THIS frame's hitlag as set by Combat, which runs after
/// Fsm; observing the frame-start hitlag in the Fsm scan would tick the meter for a fighter hit
/// this frame (Combat skips it) and skip it for a fighter whose hitlag expired (Combat ticks it).
/// `np` uses `min(MAX_PLAYERS)` to match the old `update_items` bound exactly.
pub(crate) fn tick_badge_meters(n: &mut SimState) {
    let np = (n.active as usize).min(MAX_PLAYERS);
    // AC fuel meter (plans/ac-ship-backlog.md item 1): badge wear, so it ticks with the badge arm
    // rather than in the FSM's per-frame block alongside `arm_cd`/`qb_cd`. Paused during hitlag the
    // same way those cooldowns are; at 0 it clears ONLY the AcCore bit, other badges untouched.
    for fi in 0..np {
        let f = &mut n.fighters[fi];
        if f.has_badge(Badge::AcCore) && f.hitlag == 0 && f.ac_gas > 0 {
            f.ac_gas -= 1;
            if f.ac_gas == 0 {
                f.badges &= !(Badge::AcCore as u8); // out of gas: pop back out of the mech
            }
        }
        wear::tick(f); // generic per-badge wear timer (queue-2026-07-03 item 6); AcCore opts out
    }
}
