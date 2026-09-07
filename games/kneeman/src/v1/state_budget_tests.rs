//! State-size budget: the rollback state's byte cost, pinned so growth is caught the frame
//! it happens, not discovered in a profiler months later.
//!
//! Two costs, both paid per path:
//!   - IN-MEMORY (`size_of`): ggrs savestates `SimState` (it's `Copy`) up to ~8x/frame during
//!     rollback. Every byte here is copied on every savestate.
//!   - WIRE (`state_size_bytes`, bincode fixint LE): the net checksum + resume-snapshot cost.
//!     Larger than in-memory because bincode widens each enum discriminant to 4 bytes.
//!
//! `InkPath` used to carry its geometry inline (`pts`/`class`/`born` arrays sized to
//! `MAX_PATH_PTS`) and dominated the state on its own. The ink-storage-arena migration
//! (plans/ink-storage-arena.md) turned it into a `{start, len}` handle into the shared
//! `SimState.nodes: [InkNode; NODE_POOL]` pool: geometry storage is now decoupled from path
//! COUNT, so adding a path costs one handle (~120 B), not a full point reservation. The pool
//! (sized to aggregate on-screen ink, not `MAX_DRAWN x MAX_PATH_PTS`) is now the dominant
//! array instead. This file encodes the REALIZED layout as executable asserts, not prose
//! estimates. When a pinned number changes DELIBERATELY, update the const AND the changelog
//! note; an ACCIDENTAL change trips here.

#[cfg(test)]
mod state_budget {
    use crate::v1::arena::NODE_POOL;
    use crate::v1::stage::{InkPath, MAX_DRAWN};
    use crate::v1::{Fighter, InkNode, Item, MAX_ITEMS, MAX_PLAYERS, SimState, state_size_bytes};
    use core::mem::size_of;

    // ── pinned sizes (re-measured 2026-07-07, native x86_64/arm64; no pointer-width fields in the
    //    state, so these are identical on wasm32) -- bumped for the ink-ceiling raise MAX_DRAWN
    //    48->128 / NODE_POOL 640->1600: SimState/wire roughly doubled because paths[] and nodes[]
    //    both grew (was SimState=21_832, wire=23_312 at 48/640). The per-path handle (INKPATH_MEM)
    //    is unchanged -- the arena migration decoupled geometry from path COUNT, so more slots only
    //    add 120 B handles, not geometry (was InkPath=384, SimState=26_624, wire=30_186 pre-arena;
    //    520/33_152/36_714 before the born[] shrink before that) ──
    // 2026-09-06: optional 24-byte terrain identity/durability/break metadata on each path
    // and item adds 6,144 resident bytes. Empty options add 256 bytes to the spawn wire image;
    // live cells additionally encode their metadata. Geometry still uses the existing arena.
    const INKPATH_MEM: usize = 144;
    const FIGHTER_MEM: usize = 408;
    const SIMSTATE_MEM: usize = 49_416;
    // Special-entry context adds one serialized bool per fighter; resident padding absorbs it.
    const WIRE_BYTES: u64 = 46_692;

    #[test]
    fn pinned_sizes_hold() {
        assert_eq!(
            size_of::<InkPath>(),
            INKPATH_MEM,
            "InkPath size changed. Each byte costs MAX_DRAWN({MAX_DRAWN}) x ~8 savestates/frame. \
             If deliberate, re-pin INKPATH_MEM and note the SimState delta."
        );
        assert_eq!(
            size_of::<Fighter>(),
            FIGHTER_MEM,
            "Fighter size changed (x MAX_PLAYERS)."
        );
        assert_eq!(
            size_of::<SimState>(),
            SIMSTATE_MEM,
            "SimState size changed; re-pin deliberately."
        );
    }

    #[test]
    fn wire_size_is_fixed_regardless_of_active_fighters() {
        // Dormant slots still encode, so roster size does not affect this spawn baseline.
        // Active terrain-cell metadata adds payload beyond this baseline.
        assert_eq!(state_size_bytes(&SimState::spawn()), WIRE_BYTES);
        assert_eq!(state_size_bytes(&SimState::spawn_n(4)), WIRE_BYTES);
        assert_eq!(state_size_bytes(&SimState::spawn_n(1)), WIRE_BYTES);
    }

    #[test]
    fn adding_a_path_costs_exactly_one_inkpath() {
        // No inter-element padding: the memory to add one slot is precisely one InkPath, so the
        // growth model below is exact, not approximate. Note this is now just the HANDLE cost --
        // the geometry itself is a separate, shared-pool cost (see `inkpath_is_a_handle_not_a_reservation`).
        assert_eq!(
            size_of::<[InkPath; MAX_DRAWN]>(),
            MAX_DRAWN * size_of::<InkPath>()
        );
    }

    #[test]
    fn nodes_dominate_the_state() {
        // Post-arena, `paths[]` no longer dominates -- it shrank to a handle array. The shared
        // `nodes[]` pool (geometry for every path, drawn or baked) is now the largest fixed array
        // in `SimState`, ahead of `items[]`, `fighters[]`, and `paths[]` itself. Any ceiling
        // decision on ink volume is really a decision about the pool size (`NODE_POOL`), not
        // `MAX_DRAWN`.
        let nodes = NODE_POOL * size_of::<InkNode>();
        let paths = MAX_DRAWN * size_of::<InkPath>();
        let items = MAX_ITEMS * size_of::<Item>();
        let fighters = MAX_PLAYERS * size_of::<Fighter>();
        assert!(
            nodes > paths,
            "nodes[] ({nodes} B) no longer beats paths[] ({paths} B)"
        );
        assert!(
            nodes > items,
            "nodes[] ({nodes} B) no longer beats items[] ({items} B)"
        );
        assert!(
            nodes > fighters,
            "nodes[] ({nodes} B) no longer beats fighters[] ({fighters} B)"
        );
        let pct = nodes * 100 / size_of::<SimState>();
        assert!(
            pct >= 30,
            "nodes[] is only {pct}% of state; the dominance assumption broke"
        );
    }

    /// Unordered pair count of the `resolve_ink_billiard` double loop (`for i in 0..N, j in i+1..N`).
    /// This is O(N^2): the reason a naive MAX_DRAWN bump past ~100 needs a broadphase, not just RAM.
    fn billiard_pairs(n: usize) -> usize {
        n * (n - 1) / 2
    }

    /// Projected in-memory SimState if `InkPath` were `path_bytes` and there were `slots` of them,
    /// holding everything else constant. Lets a shrink/grow proposal be checked before it's built.
    /// Post-arena this only models the HANDLE array; `nodes[]` (the geometry pool) is sized
    /// independently by `NODE_POOL`, not by `MAX_DRAWN`.
    fn projected_state_mem(slots: usize, path_bytes: usize) -> usize {
        let rest = SIMSTATE_MEM - MAX_DRAWN * INKPATH_MEM; // fighters, items, helm, fx, nodes, scalars
        rest + slots * path_bytes
    }

    #[test]
    fn growth_model_is_documented() {
        // Memory grows LINEARLY in the count; the billiard pass grows QUADRATICALLY. MAX_DRAWN is
        // now 128, so the MAX_DRAWN line and the old standalone "128" line coincide -- kept as one,
        // with 256 as a >MAX_DRAWN projection illustrating the quadratic climb. The (since removed) broadphase
        // (d72066c) is what keeps `resolve_ink_billiard` from actually paying these pair counts at
        // the higher slot ceiling.
        assert_eq!(billiard_pairs(MAX_DRAWN), 8128); // 128 slots -> 8128 pairs/frame (was 1128 at 48)
        assert_eq!(billiard_pairs(256), 32_640); // 4.0x the pairs at 2x the slots (quadratic)

        // At today's (post-arena) InkPath size, growing the HANDLE count costs (holding InkPath
        // fixed; `nodes[]` is unaffected since it's sized by `NODE_POOL`, not `MAX_DRAWN`):
        assert_eq!(projected_state_mem(MAX_DRAWN, INKPATH_MEM), SIMSTATE_MEM); // identity check
        assert_eq!(projected_state_mem(256, INKPATH_MEM), 30_984 + 256 * 144);
    }

    #[test]
    fn inkpath_is_a_handle_not_a_reservation() {
        // REALIZED, not projected: `InkPath` no longer carries `pts`/`class`/`born` arrays sized
        // to `MAX_PATH_PTS` -- it's a `{start: u16, len: u8}` handle into the shared `nodes` pool
        // plus its rigid-body/props fields. This asserts the handle shrink actually landed and
        // that the geometry moved to a array dominant over any single path's own record.
        assert!(
            size_of::<InkPath>() < 200,
            "InkPath grew past a handle's expected footprint"
        );
        assert_eq!(INKPATH_MEM, size_of::<InkPath>());

        let nodes = size_of::<[InkNode; NODE_POOL]>();
        assert!(
            nodes > size_of::<InkPath>() * MAX_DRAWN,
            "the shared nodes[] pool ({nodes} B) should dwarf the whole paths[] handle array"
        );
    }
}
