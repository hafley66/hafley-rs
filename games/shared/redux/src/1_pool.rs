//! Generic deterministic arena: a fixed pool of `T` plus a first-fit free-span allocator handing
//! out `{start,len}` [`Handle`]s. Any Copy(+Serialize) payload with the shape "many owners share
//! one backing store, sized to AGGREGATE demand instead of per-owner worst case" reuses this one
//! battle-tested allocator instead of re-deriving it per feature.
//!
//! `Pool<T, N, CAP>` promotes the payload type, slot count, and free-span capacity to generic
//! parameters. Allocation policy stays separate from caller-specific sizing and eviction policy.
//!
//! ## Determinism
//! The allocator is a PURE function of `self`: first-fit over spans stored in ascending-start
//! order, no heap, no hashing, no address-derived ordering. Two peers replaying the same
//! alloc/free sequence get byte-identical state, which is the rollback invariant. To keep derived
//! `PartialEq` canonical (so two allocators representing the same free set compare `==`), every
//! removal zeroes the vacated tail slot -- the array beyond `count` is always `Span{0,0}`.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A `{start,len}` handle into a [`Pool`]'s backing slots. Meaningless without the pool it
/// indexes. Callers thread the owning pool back to every read and write.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Handle {
    pub start: u32,
    pub len: u32,
}

impl Handle {
    /// The empty handle: zero-length, indexes nothing. Distinct from "unallocated" only by
    /// convention -- callers that need to distinguish "never allocated" from "allocated zero
    /// slots" should track that separately.
    pub const EMPTY: Self = Self { start: 0, len: 0 };

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// A contiguous free range `[start, start + len)` of a pool.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct Span {
    start: u32,
    len: u32,
}

const EMPTY_SPAN: Span = Span { start: 0, len: 0 };

/// Deterministic first-fit free-span allocator over `[0, len)` (the `len` passed to
/// [`FreeSpans::new`], so one type serves any pool size). `CAP` bounds the number of free
/// fragments the caller's allocation pattern can produce: with at most `K` simultaneous live
/// allocations the pool splits into at most `K + 1` free spans (one between/around each live
/// allocation), so size `CAP >= K + 1` for your own `K`. Fixed-cap, no heap, no hashing.
///
/// INVARIANT (maintained by every op): the first `count` spans are sorted ascending by `start`,
/// pairwise NON-ADJACENT (any two touching spans are always coalesced into one), every span has
/// `len > 0`, the tail `[count, CAP)` is all `Span{0,0}`, and
/// `sum(free.len) + sum(allocated) == len` (the `len` given to `new`).
#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FreeSpans<const CAP: usize> {
    // serde's own array impls stop at len 32; BigArray
    // covers any CAP, including <= 32, so it is always safe to route through it here.
    #[cfg_attr(feature = "serde", serde(with = "serde_big_array::BigArray"))]
    spans: [Span; CAP],
    count: u32,
}

impl<const CAP: usize> FreeSpans<CAP> {
    /// Fresh allocator over `[0, len)`. `count == 1` when `len > 0`, `count == 0` (nothing free)
    /// when `len == 0`. `CAP == 0` makes every `alloc` of a nonzero pool fail immediately (no
    /// space to record the initial free span) -- callers should size `CAP >= 1`.
    pub const fn new(len: u32) -> Self {
        let mut spans = [EMPTY_SPAN; CAP];
        let count = if len > 0 && CAP > 0 {
            spans[0] = Span { start: 0, len };
            1
        } else {
            0
        };
        FreeSpans { spans, count }
    }

    /// Carve `n` slots off the pool. First-fit: scan spans in stored (ascending-start) order,
    /// take the FIRST with `len >= n`, carve `n` off its FRONT, and return the carved start.
    /// `None` if nothing fits (pool full, too fragmented, or `n == 0` on an empty pool is still a
    /// valid zero-width handle at whatever `start` the first span offers) -- on `None` the
    /// allocator is left UNTOUCHED, so a failed alloc never partially corrupts prior allocations.
    pub fn alloc(&mut self, n: u32) -> Option<u32> {
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

    /// Return `[start, start + n)` to the pool, inserting at its sorted position and COALESCING
    /// with any adjacent free span on either side (merge when `prev.start + prev.len == start` or
    /// `start + n == next.start`). Never leaves two adjacent free spans. `n == 0` is a no-op.
    pub fn free(&mut self, start: u32, n: u32) {
        if n == 0 {
            return;
        }
        // Sorted insert point: first span whose start is >= ours. Frees are of disjoint ranges,
        // so `spans[idx].start == start` never happens for a well-behaved caller.
        let mut idx = 0;
        while idx < self.count as usize && self.spans[idx].start < start {
            idx += 1;
        }

        let mut merged = Span { start, len: n };

        // Coalesce with the RIGHT neighbor (at `idx`) if we touch its front. Remove it; `idx`
        // still indexes the same logical gap and the left neighbor at `idx-1` is unaffected by
        // the removal.
        if idx < self.count as usize && merged.start + merged.len == self.spans[idx].start {
            merged.len += self.spans[idx].len;
            self.remove(idx);
        }

        // Coalesce with the LEFT neighbor (at `idx-1`) if it touches our front. Extend it in
        // place -- no new fragment, so no insert.
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
    /// conservation invariant `free_total() + allocated == len`.
    pub fn free_total(&self) -> u64 {
        self.spans[..self.count as usize]
            .iter()
            .map(|span| span.len as u64)
            .sum()
    }

    /// Number of free fragments. `1` on a fresh (nonzero-length) or fully-reclaimed pool.
    pub fn frag_count(&self) -> usize {
        self.count as usize
    }

    /// Shift entries `[idx+1, count)` left over `idx`, drop the count, and zero the vacated tail
    /// slot so the array beyond `count` stays canonical `Span{0,0}` (keeps derived `PartialEq`
    /// meaningful).
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

    /// Shift entries `[idx, count)` right to open a hole at `idx`, then write `span`.
    /// Debug-asserts the `CAP` fragment cap: overflow means the caller's own "at most K live
    /// allocations" invariant broke upstream (in a release build the write past the cap would
    /// corrupt, so it is a bug either way -- the assert makes it loud in tests).
    fn insert(&mut self, idx: usize, span: Span) {
        debug_assert!(
            (self.count as usize) < CAP,
            "free-span array overflow: more fragments than CAP -- upstream over-allocated"
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

/// A fixed pool of `N` `T` slots plus its own [`FreeSpans<CAP>`] allocator. `CAP` bounds the
/// number of live non-adjacent allocations the pool can hold simultaneously; size it to your own
/// "at most K live allocations" invariant as `CAP >= K + 1` (see [`FreeSpans`]).
///
/// `T` need only be `Copy`; no `Default` bound (the pool is filled with a caller-supplied value at
/// construction, providing a canonical blank value for unused slots).
#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))
)]
pub struct Pool<T, const N: usize, const CAP: usize> {
    #[cfg_attr(feature = "serde", serde(with = "serde_big_array::BigArray"))]
    slots: [T; N],
    free: FreeSpans<CAP>,
}

impl<T: Copy, const N: usize, const CAP: usize> Pool<T, N, CAP> {
    /// A fresh pool: every slot holds `fill`, the whole range `[0, N)` is free.
    pub fn new(fill: T) -> Self {
        Self {
            slots: [fill; N],
            free: FreeSpans::new(N as u32),
        }
    }

    /// Carve `n` slots off the pool; see [`FreeSpans::alloc`]. The returned handle's slots hold
    /// whatever this pool's most recent occupant of that range left behind -- callers that need a
    /// blank read should overwrite before reading, same as any other arena.
    pub fn alloc(&mut self, n: u32) -> Option<Handle> {
        self.free.alloc(n).map(|start| Handle { start, len: n })
    }

    /// Return `handle`'s range to the pool; see [`FreeSpans::free`].
    pub fn free(&mut self, handle: Handle) {
        self.free.free(handle.start, handle.len);
    }

    pub fn get(&self, handle: Handle) -> &[T] {
        &self.slots[handle.start as usize..(handle.start + handle.len) as usize]
    }

    pub fn get_mut(&mut self, handle: Handle) -> &mut [T] {
        &mut self.slots[handle.start as usize..(handle.start + handle.len) as usize]
    }

    pub fn frag_count(&self) -> usize {
        self.free.frag_count()
    }

    pub fn free_total(&self) -> u64 {
        self.free.free_total()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LEN: u32 = 100;
    type Sut = FreeSpans<8>;

    /// Sum of free lengths plus the caller-tracked allocated total must equal the whole pool. The
    /// core rollback/checksum invariant: no slot is both free and allocated, none is lost.
    fn assert_conserved(free: &Sut, allocated: u64) {
        assert_eq!(
            free.free_total() + allocated,
            TEST_LEN as u64,
            "conservation broken: {} free + {} allocated != {}",
            free.free_total(),
            allocated,
            TEST_LEN
        );
    }

    // 1. fresh -> alloc(k) returns 0; a second alloc(k) returns k (front carving).
    #[test]
    fn front_carving_is_sequential() {
        let mut free = Sut::new(TEST_LEN);
        assert_eq!(free.alloc(10), Some(0));
        assert_eq!(free.alloc(10), Some(10));
        assert_eq!(free.frag_count(), 1); // one remaining free tail span
        assert_conserved(&free, 20);
    }

    // 2. free then alloc reuses the freed range.
    #[test]
    fn free_then_alloc_reuses_range() {
        let mut free = Sut::new(TEST_LEN);
        let first = free.alloc(10).unwrap();
        assert_eq!(first, 0);
        free.free(first, 10);
        // Reclaimed to the front, so first-fit hands 0 back out.
        assert_eq!(free.alloc(10), Some(0));
        assert_conserved(&free, 10);
    }

    // 3. fragmentation: alloc a,b,c contiguous; free the middle; a fitting alloc lands in the
    //    gap; a larger alloc skips it (first-fit) and lands after c.
    #[test]
    fn first_fit_uses_gap_then_skips_when_too_big() {
        let mut free = Sut::new(TEST_LEN);
        let a_start = free.alloc(10).unwrap(); // [0,10)
        let b_start = free.alloc(10).unwrap(); // [10,20)
        let c_start = free.alloc(10).unwrap(); // [20,30)
        assert_eq!((a_start, b_start, c_start), (0, 10, 20));

        free.free(b_start, 10); // gap [10,20) + tail [30, TEST_LEN)

        // A size-6 alloc fits the front gap -> lands at 10.
        assert_eq!(free.alloc(6), Some(10)); // gap now [16,20)
        // A size-8 alloc no longer fits the 4-wide gap remnant -> skips to the tail (after c, at
        // 30).
        assert_eq!(free.alloc(8), Some(30));
        // a(10) + c(10) still held, b freed, plus the two gap/tail allocs (6 + 8).
        assert_conserved(&free, 10 + 10 + 6 + 8);
    }

    // 4. coalesce: after freeing everything, count == 1 and the single span is [0, TEST_LEN).
    #[test]
    fn full_reclaim_collapses_to_one_span() {
        let mut free = Sut::new(TEST_LEN);
        let a_start = free.alloc(10).unwrap();
        let b_start = free.alloc(20).unwrap();
        let c_start = free.alloc(30).unwrap();
        // Free out of order to exercise both-side coalescing.
        free.free(b_start, 20);
        free.free(a_start, 10);
        free.free(c_start, 30);
        assert_eq!(free.frag_count(), 1);
        assert_eq!(free.alloc(TEST_LEN), Some(0)); // the whole pool is one span again
        assert_conserved(&free, TEST_LEN as u64);
    }

    // 5. adjacency: freeing two ranges that touch yields ONE span, not two.
    #[test]
    fn touching_frees_merge_into_one() {
        let mut free = Sut::new(TEST_LEN);
        let a_start = free.alloc(10).unwrap(); // [0,10)
        let b_start = free.alloc(10).unwrap(); // [10,20)
        let _tail_keeps_count_at_2 = free.alloc(5).unwrap(); // [20,25) stays allocated
        assert_eq!(free.frag_count(), 1); // only the [25, TEST_LEN) tail is free

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

    // 6. pool-full: allocations summing past TEST_LEN eventually return None, and the already-
    //    allocated ranges are untouched (no partial corruption).
    #[test]
    fn pool_full_returns_none_without_corruption() {
        let mut free = Sut::new(TEST_LEN);
        let big = free.alloc(TEST_LEN - 4).unwrap();
        assert_eq!(big, 0);
        // Only 4 slots remain; a size-10 request cannot fit.
        let before = free;
        assert_eq!(free.alloc(10), None);
        assert_eq!(
            free, before,
            "failed alloc must leave the allocator byte-identical"
        );
        // The 4-slot remainder is still allocatable.
        assert_eq!(free.alloc(4), Some(TEST_LEN - 4));
        assert_eq!(free.alloc(1), None);
        assert_conserved(&free, TEST_LEN as u64);
    }

    // 7. conservation: Sum free + Sum allocated == TEST_LEN after each op in a mixed sequence.
    #[test]
    fn conservation_holds_through_mixed_sequence() {
        let mut free = Sut::new(TEST_LEN);
        let mut allocated: u64 = 0;

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

    // 8. determinism: two fresh allocators driven by the identical alloc/free sequence compare ==.
    #[test]
    fn identical_sequences_produce_identical_state() {
        fn drive() -> Sut {
            let mut free = Sut::new(TEST_LEN);
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

    // 9. size guard: CAP(8) x Span(8 B: two u32) + count(u32, 4 B) = 68 B, no padding surprises.
    #[test]
    fn free_spans_size_is_pinned() {
        assert_eq!(core::mem::size_of::<Sut>(), 8 * 8 + 4);
    }

    // --- Pool<T, N, CAP>: the same allocator, now actually handing back typed slots. ---

    #[test]
    fn pool_alloc_get_mut_and_free_round_trip() {
        let mut pool: Pool<i32, 32, 5> = Pool::new(0);
        let a = pool.alloc(4).unwrap();
        let b = pool.alloc(8).unwrap();
        assert_eq!(a, Handle { start: 0, len: 4 });
        assert_eq!(b, Handle { start: 4, len: 8 });

        pool.get_mut(a).copy_from_slice(&[1, 2, 3, 4]);
        pool.get_mut(b).fill(9);
        assert_eq!(pool.get(a), &[1, 2, 3, 4]);
        assert_eq!(pool.get(b), &[9; 8]);

        pool.free(a);
        let c = pool.alloc(2).unwrap();
        assert_eq!(
            c,
            Handle { start: 0, len: 2 },
            "freed front is reused first-fit"
        );
        assert_eq!(pool.frag_count(), 2); // [2,4) leftover + tail past b
    }

    #[test]
    fn pool_conservation_matches_free_spans() {
        let mut pool: Pool<u8, 64, 4> = Pool::new(0);
        assert_eq!(pool.free_total(), 64);
        let h = pool.alloc(20).unwrap();
        assert_eq!(pool.free_total(), 44);
        pool.free(h);
        assert_eq!(pool.free_total(), 64);
        assert_eq!(pool.frag_count(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pool_round_trips_through_serde_json() {
        let mut pool: Pool<i32, 40, 4> = Pool::new(-1);
        let h = pool.alloc(6).unwrap();
        pool.get_mut(h).copy_from_slice(&[1, 2, 3, 4, 5, 6]);

        let json = serde_json::to_string(&pool).expect("serialize");
        let back: Pool<i32, 40, 4> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            pool, back,
            "round trip must be byte-for-byte identical for checksums"
        );
    }
}
