//! Shared-node arena + deterministic free-span allocator (plans/ink-storage-arena.md slice 4).
//!
//! Today every `InkPath` reserves `MAX_PATH_PTS` geometry slots inline, so the state carries
//! `MAX_DRAWN x MAX_PATH_PTS` node slots worst-case (=1152) that are mostly empty. This module is
//! the storage primitive that decouples path COUNT from node STORAGE: paths become `{start,len}`
//! handles into one shared `[InkNode; NODE_POOL]` pool, and `FreeSpans` hands out ranges of that pool.
//!
//! NOTHING here is wired into `SimState`/`InkPath` yet -- this slice is the primitive + its tests
//! only, so `main` stays byte-identical. The wire-in (PathHead handles, pool eviction) is the next
//! slice. Every type is `Copy + Clone + PartialEq + Serialize + Deserialize` because next slice they
//! live in `SimState`, which rolls back (ggrs savestates) and checksums.
//!
//! ## Determinism
//! The allocator is a PURE function of `self`: first-fit over spans stored in ascending-start order,
//! no heap, no hashing, no address-derived ordering. Two peers replaying the same alloc/free
//! sequence get byte-identical state, which is the rollback invariant. To keep derived `PartialEq`
//! canonical (so two allocators representing the same free set compare `==`), every removal zeroes the
//! vacated tail slot -- the array beyond `count` is always `Span{0,0}`.

use crate::v1::{MAX_DRAWN, SegClass, Vector2};
use serde::{Deserialize, Serialize};

/// One geometry node in the shared pool. Payload for a path's polyline.
///
/// Layout (measured): `pt` @0 (8 B, Vec2 aligns to 4), `born_off` @8 (2 B), `class` @10 (1 B),
/// tail pad to 12 (struct aligns to 4). `size_of::<InkNode>() == 12`, pinned by `node_is_12_bytes`.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct InkNode {
    /// Offset from the owning path's `pos` (world position = `pt + pos`).
    pub pt: Vector2,
    /// Tick offset from the path's `stroke_born` (the node's birth time).
    pub born_off: u16,
    /// Class of the segment STARTING at this node.
    pub class: SegClass,
}

impl InkNode {
    /// Blank node: the pool fill value (`spawn_n` seeds `[InkNode::ZERO; NODE_POOL]`) and what a
    /// freed slot logically holds. A `Default`-shaped const so `SimState` can const-init the pool.
    pub const ZERO: Self = Self {
        pt: Vector2::ZERO,
        born_off: 0,
        class: SegClass::None,
    };
}

impl Default for InkNode {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Shared node budget. Sized to AGGREGATE peak ink on screen, NOT `MAX_DRAWN x MAX_PATH_PTS`
/// (=1152, mostly empty). Pool-full is handled by the caller next slice (evict the oldest player
/// stroke, same policy as `claim_stroke_slot`), so undersizing degrades gracefully -- it never
/// corrupts. TUNABLE: raise once a heavy playtest measures the real simultaneous-node peak.
/// Must stay `<= u16::MAX` because node indices are `u16`.
pub const NODE_POOL: usize = 1600;

/// Max free fragments. With at most `MAX_DRAWN` live allocations the pool splits into at most
/// `MAX_DRAWN + 1` free spans (one between/around each allocation), so the backing array is sized
/// exactly to that worst case. Exceeding it means the "at most MAX_DRAWN live allocations" invariant
/// broke upstream -- `insert` debug-asserts rather than silently corrupt.
const SPAN_CAP: usize = MAX_DRAWN + 1;

/// A contiguous free range `[start, start + len)` of the node pool.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct Span {
    start: u16,
    len: u16,
}

const EMPTY_SPAN: Span = Span { start: 0, len: 0 };

/// Deterministic first-fit free-span allocator over `[0, NODE_POOL)`. Fixed-cap, no heap, no hashing.
///
/// INVARIANT (maintained by every op): the first `count` spans are sorted ascending by `start`,
/// pairwise NON-ADJACENT (any two touching spans are always coalesced into one), every span has
/// `len > 0`, the tail `[count, SPAN_CAP)` is all `Span{0,0}`, and
/// `sum(free.len) + sum(allocated) == NODE_POOL`.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FreeSpans {
    // serde's hand-rolled array impls stop at len 32; SPAN_CAP=MAX_DRAWN+1=129 is past that (same
    // BigArray shim SimState uses for its [_; MAX_DRAWN]/[_; MAX_ITEMS] fields).
    #[serde(with = "serde_big_array::BigArray")]
    spans: [Span; SPAN_CAP],
    count: u16,
}

impl Default for FreeSpans {
    fn default() -> Self {
        Self::new()
    }
}

impl FreeSpans {
    /// Fresh allocator: the whole pool is one free span `[0, NODE_POOL)`, `count == 1`.
    pub const fn new() -> Self {
        let mut spans = [EMPTY_SPAN; SPAN_CAP];
        spans[0] = Span {
            start: 0,
            len: NODE_POOL as u16,
        };
        FreeSpans { spans, count: 1 }
    }

    /// Carve `n` nodes off the pool. First-fit: scan spans in stored (ascending-start) order, take
    /// the FIRST with `len >= n`, carve `n` off its FRONT, and return the carved start. `None` if
    /// nothing fits (pool full or too fragmented) -- on `None` the allocator is left UNTOUCHED, so a
    /// failed alloc never partially corrupts prior allocations.
    pub fn alloc(&mut self, n: u16) -> Option<u16> {
        for idx in 0..self.count as usize {
            if self.spans[idx].len >= n {
                let start = self.spans[idx].start;
                self.spans[idx].start += n;
                self.spans[idx].len -= n;
                if self.spans[idx].len == 0 {
                    self.remove(idx);
                }
                return Some(start);
            }
        }
        None
    }

    /// Return `[start, start + n)` to the pool, inserting at its sorted position and COALESCING with
    /// any adjacent free span on either side (merge when `prev.start + prev.len == start` or
    /// `start + n == next.start`). Never leaves two adjacent free spans. `n == 0` is a no-op.
    pub fn free(&mut self, start: u16, n: u16) {
        if n == 0 {
            return;
        }
        // Sorted insert point: first span whose start is >= ours. Frees are of disjoint ranges, so
        // `spans[idx].start == start` never happens for a well-behaved caller.
        let mut idx = 0;
        while idx < self.count as usize && self.spans[idx].start < start {
            idx += 1;
        }

        let mut merged = Span { start, len: n };

        // Coalesce with the RIGHT neighbor (at `idx`) if we touch its front. Remove it; `idx` still
        // indexes the same logical gap and the left neighbor at `idx-1` is unaffected by the removal.
        if idx < self.count as usize && merged.start + merged.len == self.spans[idx].start {
            merged.len += self.spans[idx].len;
            self.remove(idx);
        }

        // Coalesce with the LEFT neighbor (at `idx-1`) if it touches our front. Extend it in place --
        // no new fragment, so no insert.
        if idx > 0 {
            let prev = idx - 1;
            debug_assert!(
                self.spans[prev].start + self.spans[prev].len <= merged.start,
                "free of an already-free or overlapping range (start={start}, n={n})"
            );
            if self.spans[prev].start + self.spans[prev].len == merged.start {
                self.spans[prev].len += merged.len;
                return;
            }
        }

        self.insert(idx, merged);
    }

    /// Total free capacity currently in the pool (`sum` of all free span lengths). Used by the
    /// conservation invariant `free_total() + allocated == NODE_POOL`.
    pub fn free_total(&self) -> u32 {
        let mut sum = 0u32;
        for idx in 0..self.count as usize {
            sum += self.spans[idx].len as u32;
        }
        sum
    }

    /// Number of free fragments. `1` on a fresh or fully-reclaimed pool.
    pub fn frag_count(&self) -> u16 {
        self.count
    }

    /// Shift entries `[idx+1, count)` left over `idx`, drop the count, and zero the vacated tail slot
    /// so the array beyond `count` stays canonical `Span{0,0}` (keeps derived `PartialEq` meaningful).
    fn remove(&mut self, idx: usize) {
        let last = self.count as usize - 1;
        let mut cursor = idx;
        while cursor < last {
            self.spans[cursor] = self.spans[cursor + 1];
            cursor += 1;
        }
        self.spans[last] = EMPTY_SPAN;
        self.count -= 1;
    }

    /// Shift entries `[idx, count)` right to open a hole at `idx`, then write `span`. Debug-asserts the
    /// `MAX_DRAWN + 1` fragment cap: overflow means the "at most MAX_DRAWN live allocations" invariant
    /// broke upstream (in a release build the write past the cap would corrupt, so it is a bug either
    /// way -- the assert makes it loud in tests/synctest).
    fn insert(&mut self, idx: usize, span: Span) {
        debug_assert!(
            (self.count as usize) < SPAN_CAP,
            "free-span array overflow: more than MAX_DRAWN+1 fragments -- upstream over-allocated"
        );
        let mut cursor = self.count as usize;
        while cursor > idx {
            self.spans[cursor] = self.spans[cursor - 1];
            cursor -= 1;
        }
        self.spans[idx] = span;
        self.count += 1;
    }
}

/// A standalone node pool + allocator for tests that build strokes outside a `SimState`. A path's
/// `{start,len}` handle is meaningless without the pool it indexes, so every test builder writes
/// into a `Scratch` and threads its `nodes` back into geometry reads -- exactly what `SimState`
/// does. Shared across the crate's test modules (stage/tests.rs and the crate-root `*_tests.rs`).
#[cfg(test)]
pub(crate) struct Scratch {
    pub nodes: [InkNode; NODE_POOL],
    pub free: FreeSpans,
}

#[cfg(test)]
impl Scratch {
    pub fn new() -> Self {
        Self {
            nodes: [InkNode::ZERO; NODE_POOL],
            free: FreeSpans::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    /// Sum of free lengths plus the caller-tracked allocated total must equal the whole pool. The
    /// core rollback/checksum invariant: no node is both free and allocated, none is lost.
    fn assert_conserved(free: &FreeSpans, allocated: u32) {
        assert_eq!(
            free.free_total() + allocated,
            NODE_POOL as u32,
            "conservation broken: {} free + {} allocated != {}",
            free.free_total(),
            allocated,
            NODE_POOL
        );
    }

    // 1. fresh -> alloc(k) returns 0; a second alloc(k) returns k (front carving).
    #[test]
    fn front_carving_is_sequential() {
        let mut free = FreeSpans::new();
        assert_eq!(free.alloc(10), Some(0));
        assert_eq!(free.alloc(10), Some(10));
        assert_eq!(free.frag_count(), 1); // one remaining free tail span
        assert_conserved(&free, 20);
    }

    // 2. free then alloc reuses the freed range.
    #[test]
    fn free_then_alloc_reuses_range() {
        let mut free = FreeSpans::new();
        let first = free.alloc(10).unwrap();
        assert_eq!(first, 0);
        free.free(first, 10);
        // Reclaimed to the front, so first-fit hands 0 back out.
        assert_eq!(free.alloc(10), Some(0));
        assert_conserved(&free, 10);
    }

    // 3. fragmentation: alloc a,b,c contiguous; free the middle; a fitting alloc lands in the gap; a
    //    larger alloc skips it (first-fit) and lands after c.
    #[test]
    fn first_fit_uses_gap_then_skips_when_too_big() {
        let mut free = FreeSpans::new();
        let a_start = free.alloc(10).unwrap(); // [0,10)
        let b_start = free.alloc(10).unwrap(); // [10,20)
        let c_start = free.alloc(10).unwrap(); // [20,30)
        assert_eq!((a_start, b_start, c_start), (0, 10, 20));

        free.free(b_start, 10); // gap [10,20) + tail [30, NODE_POOL)

        // A size-6 alloc fits the front gap -> lands at 10.
        assert_eq!(free.alloc(6), Some(10)); // gap now [16,20)
        // A size-8 alloc no longer fits the 4-wide gap remnant -> skips to the tail (after c, at 30).
        assert_eq!(free.alloc(8), Some(30));
        // a(10) + c(10) still held, b freed, plus the two gap/tail allocs (6 + 8).
        assert_conserved(&free, 10 + 10 + 6 + 8);
    }

    // 4. coalesce: after freeing everything, count == 1 and the single span is [0, NODE_POOL).
    #[test]
    fn full_reclaim_collapses_to_one_span() {
        let mut free = FreeSpans::new();
        let a_start = free.alloc(10).unwrap();
        let b_start = free.alloc(20).unwrap();
        let c_start = free.alloc(30).unwrap();
        // Free out of order to exercise both-side coalescing.
        free.free(b_start, 20);
        free.free(a_start, 10);
        free.free(c_start, 30);
        assert_eq!(free.frag_count(), 1);
        assert_eq!(free.alloc(NODE_POOL as u16), Some(0)); // the whole pool is one span again
        assert_conserved(&free, NODE_POOL as u32);
    }

    // 5. adjacency: freeing two ranges that touch yields ONE span, not two.
    #[test]
    fn touching_frees_merge_into_one() {
        let mut free = FreeSpans::new();
        let a_start = free.alloc(10).unwrap(); // [0,10)
        let b_start = free.alloc(10).unwrap(); // [10,20)
        let _tail_keeps_count_at_2 = free.alloc(5).unwrap(); // [20,25) stays allocated
        assert_eq!(free.frag_count(), 1); // only the [25, NODE_POOL) tail is free

        free.free(a_start, 10); // now: free [0,10) + tail
        assert_eq!(free.frag_count(), 2);
        free.free(b_start, 10); // [10,20) touches [0,10) front -> must merge, not add a fragment
        assert_eq!(
            free.frag_count(),
            2,
            "adjacent frees must coalesce into [0,20) + tail"
        );
        assert_conserved(&free, 5);
    }

    // 6. pool-full: allocations summing past NODE_POOL eventually return None, and the already-
    //    allocated ranges are untouched (no partial corruption).
    #[test]
    fn pool_full_returns_none_without_corruption() {
        let mut free = FreeSpans::new();
        let big = free.alloc(NODE_POOL as u16 - 4).unwrap();
        assert_eq!(big, 0);
        // Only 4 nodes remain; a size-10 request cannot fit.
        let before = free;
        assert_eq!(free.alloc(10), None);
        assert_eq!(
            free, before,
            "failed alloc must leave the allocator byte-identical"
        );
        // The 4-node remainder is still allocatable.
        assert_eq!(free.alloc(4), Some(NODE_POOL as u16 - 4));
        assert_eq!(free.alloc(1), None);
        assert_conserved(&free, NODE_POOL as u32);
    }

    // 7. conservation: Sum free + Sum allocated == NODE_POOL after each op in a mixed sequence.
    #[test]
    fn conservation_holds_through_mixed_sequence() {
        let mut free = FreeSpans::new();
        let mut allocated: u32 = 0;

        let handle_a = free.alloc(24).unwrap();
        allocated += 24;
        assert_conserved(&free, allocated);

        let handle_b = free.alloc(12).unwrap();
        allocated += 12;
        assert_conserved(&free, allocated);

        free.free(handle_a, 24);
        allocated -= 24;
        assert_conserved(&free, allocated);

        let handle_c = free.alloc(30).unwrap();
        allocated += 30;
        assert_conserved(&free, allocated);

        free.free(handle_b, 12);
        allocated -= 12;
        assert_conserved(&free, allocated);

        free.free(handle_c, 30);
        allocated -= 30;
        assert_conserved(&free, allocated);

        assert_eq!(free.frag_count(), 1);
        assert_eq!(allocated, 0);
    }

    // 8. determinism: two fresh FreeSpans driven by the identical alloc/free sequence compare ==.
    #[test]
    fn identical_sequences_produce_identical_state() {
        fn drive() -> FreeSpans {
            let mut free = FreeSpans::new();
            let first = free.alloc(15).unwrap();
            let second = free.alloc(9).unwrap();
            let third = free.alloc(24).unwrap();
            free.free(second, 9);
            let fourth = free.alloc(5).unwrap();
            free.free(first, 15);
            free.free(third, 24);
            free.free(fourth, 5);
            let _reuse = free.alloc(3).unwrap();
            free
        }
        assert_eq!(drive(), drive());
    }

    // 9. size guard: pin the real byte sizes. InkNode is 12 (Vec2 8 + u16 2 + enum 1, pad to 12).
    #[test]
    fn node_is_12_bytes() {
        assert_eq!(
            size_of::<InkNode>(),
            12,
            "InkNode grew past 12 B -- it multiplies by NODE_POOL in state"
        );
        // FreeSpans: SPAN_CAP(129) x Span(4 B) + count(u16 2 B) = 518, align 2. Pinned so a bump to
        // MAX_DRAWN (which grows SPAN_CAP) is a deliberate, visible state-cost change.
        assert_eq!(size_of::<FreeSpans>(), (MAX_DRAWN + 1) * 4 + 2);
        assert_eq!(size_of::<Span>(), 4);
    }
}
