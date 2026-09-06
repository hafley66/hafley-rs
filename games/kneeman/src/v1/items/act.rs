//! `ItemAct` -- what one item tick may do to a fighter, as a descriptor (plans/core-rx-refactor.md
//! section 4). Mirrors `Act`: the item pass decides WHAT purely and pushes it here; the ONE applier
//! (`acts::ApplyItemActs`, outside `items/`) carries it out on the shared fighter array. Keeping the
//! write off the item pass is what lets `items/` shrink to fighter-reads only.

use crate::v1::combat::Aim;
use crate::v1::moves::Hitbox;
use crate::v1::{Badge, FxKind, MAX_ITEMS, Vector2};

/// A fighter effect an item tick emitted. Copy + small: the catch/attach variants ride in the
/// frame-local `ItemActs` batch (applied by `ApplyItemActs`); `Strike` is applied immediately at
/// its site through the same applier (`acts::apply_item_act`), keeping strike timing bit-identical
/// while the WRITE lives outside `items/`. None enters `SimState`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ItemAct {
    None,
    /// A free-handed live grab snatched a thrown item out of the air (item.rs catch pass).
    Caught {
        fighter: i8,
        item: i8,
    },
    /// A touched auto-attach core granted a badge + rolled the arm weapon (item.rs attach arm).
    AttachBadge {
        fighter: i8,
        badge: Badge,
        arm: u8,
    },
    /// An item hitbox connected with a fighter: replay the strike on apply (victim percent is read
    /// then, so same-frame multi-hits accumulate in order). Falloff/aim are resolved by the emitter
    /// (item.rs/behavior.rs); only the WRITE crosses here. `follow` is the strike-result-gated tail.
    Strike {
        fighter: i8,
        hb: Hitbox,
        dmg: f32,
        kb_scale: f32,
        hitlag_bonus: i64,
        interrupt: bool,
        aim: Aim,
        from: Vector2,
        follow: StrikeFollow,
    },
}

/// The strike-result-gated tail an item strike carries: it fires only when the strike connected
/// (`hit.is_some()`), so it must ride with the deferred/relocated strike rather than staying at the
/// call site (which no longer sees the result).
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum StrikeFollow {
    None,
    /// Bump the struck fighter's arm-hit counter (bomb blast).
    ArmHits,
    /// Push a cosmetic fx at `at` (projectile on-hit spark).
    Fx {
        kind: FxKind,
        at: Vector2,
    },
}

/// Upper bound on item-acts per tick. Catch + attach are each at most one per item slot, so
/// `MAX_ITEMS` covers this phase; the strike variant (a later phase) will re-pin this.
pub const MAX_ITEM_ACTS: usize = MAX_ITEMS;

/// Bounded Copy batch, fx-ring style: fixed array + count, overflow refused (drop + debug_assert).
/// Born in `update_items`'s Output, consumed by `ApplyItemActs`, dead inside `step`.
#[derive(Copy, Clone)]
pub struct ItemActs {
    pub slots: [ItemAct; MAX_ITEM_ACTS],
    pub len: u8,
}

impl ItemActs {
    pub const EMPTY: Self = Self {
        slots: [ItemAct::None; MAX_ITEM_ACTS],
        len: 0,
    };

    /// Append one act; a full batch refuses the write (debug_assert fires; release drops it).
    pub fn push(&mut self, act: ItemAct) {
        let n = self.len as usize;
        if n < MAX_ITEM_ACTS {
            self.slots[n] = act;
            self.len += 1;
        } else {
            debug_assert!(false, "ItemActs overflow: {act:?} dropped");
        }
    }

    /// The live prefix (`0..len`); dormant slots are never handed out.
    pub fn as_slice(&self) -> &[ItemAct] {
        &self.slots[..self.len as usize]
    }
}
