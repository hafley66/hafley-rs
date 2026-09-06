//! Ink-board slot policy: who gets a path slot when a new stroke needs one.
//! Shared by the pen/ink-gun draw start (stage) and the tetris fire paths (items),
//! which previously disagreed -- the draw start silently dropped the stroke on a full
//! board while tetris evicted, so guns "sometimes didn't emit" and landed pieces
//! vanished under later shots.

use crate::v1::arena::InkNode;
use crate::v1::stage::{InkPath, MAX_DRAWN};

/// Claim a slot for a NEW stroke: a free slot if any, else evict the oldest settled
/// player stroke (see `oldest_evictable`). `None` = every slot is untouchable; the caller
/// keeps its ammo and the shot never happens.
// parity(v1-ink-board-eviction): new strokes take a free path slot or evict the oldest settled player stroke, preferring expiring ink while preserving baked, drawing, and traveling paths
pub(crate) fn claim_stroke_slot(paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode]) -> Option<usize> {
    if let Some(free) = paths
        .iter()
        .position(|path| !path.active() && !path.drawing)
    {
        return Some(free);
    }
    oldest_evictable(paths, nodes)
}

/// The slot to evict when something needs room -- a path SLOT (`claim_stroke_slot`) or POOL space
/// (`alloc_draw_span`): the oldest settled player stroke, EXPIRING strokes (stroke_life >= 0, pen
/// doodles) first and PERMANENT ones (tetris pieces, stroke_life < 0) only as the last resort, so
/// laid terrain outlives doodles instead of dying to them. Baked stage strokes (owner < 0),
/// mid-draw strokes, and traveling bodies are never evicted. `node_born(0)` orders by age, so it
/// reads the pool. `None` = nothing evictable.
pub(crate) fn oldest_evictable(paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode]) -> Option<usize> {
    let evictable =
        |path: &InkPath| path.active() && !path.drawing && !path.traveling() && path.owner >= 0;
    let oldest_where = |permanent: bool| {
        paths
            .iter()
            .enumerate()
            .filter(|(_, path)| evictable(path) && (path.props.stroke_life < 0) == permanent)
            .min_by_key(|(_, path)| path.node_born(0, nodes))
            .map(|(slot, _)| slot)
    };
    oldest_where(false).or_else(|| oldest_where(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::Vector2;
    use crate::v1::arena::NODE_POOL;
    use crate::v1::stage::StrokeProps;

    /// A settled 1-segment player stroke born at `tick`. `claim_stroke_slot` only reads
    /// `node_born(0) == stroke_born + born_off[start]`; with a fresh `ZERO` pool every `born_off` is
    /// 0, so pointing every stroke at `start = 0` makes `node_born(0) == tick` -- the exact age key
    /// the old inline-geometry helper produced. No real span is needed for a slot-policy test.
    fn settled_stroke(tick: u64, props: StrokeProps) -> InkPath {
        let mut path = InkPath::EMPTY;
        path.start = 0;
        path.stroke_born = tick;
        path.len = 2;
        path.props = props;
        path.owner = 0;
        path.mass = 1.0;
        path
    }

    fn zero_pool() -> [InkNode; NODE_POOL] {
        [InkNode::ZERO; NODE_POOL]
    }

    #[test]
    fn free_slot_wins_before_any_eviction() {
        let nodes = zero_pool();
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];
        paths[0] = settled_stroke(5, StrokeProps::PEN);
        assert_eq!(
            claim_stroke_slot(&paths, &nodes),
            Some(1),
            "first empty slot"
        );
    }

    #[test]
    fn full_board_evicts_the_oldest_expiring_stroke_never_a_permanent_one() {
        // NB: PEN and TETRIS presets are BOTH stroke_life -1 (permanent) today; the
        // expiring tier exists for timed-ink rows (panel stroke-life slider).
        let nodes = zero_pool();
        let timed_ink = StrokeProps {
            stroke_life: 600,
            ..StrokeProps::PEN
        };
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];
        for (slot, path) in paths.iter_mut().enumerate() {
            // even slots permanent (tetris), odd slots timed doodles; older = lower tick
            let props = if slot % 2 == 0 {
                StrokeProps::TETRIS
            } else {
                timed_ink
            };
            *path = settled_stroke(slot as u64, props);
        }
        // slot 0 (tick 0) is the oldest overall but PERMANENT: the oldest EXPIRING
        // stroke is slot 1 (tick 1), and that is who dies.
        assert_eq!(claim_stroke_slot(&paths, &nodes), Some(1));
    }

    #[test]
    fn all_permanent_board_still_yields_a_slot_as_last_resort() {
        let nodes = zero_pool();
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];
        for (slot, path) in paths.iter_mut().enumerate() {
            // Base off MAX_DRAWN so higher slots stay older (lower tick) without underflowing when
            // MAX_DRAWN exceeds the old 48 (a literal 100 base wrapped once slots reached 128).
            *path = settled_stroke(MAX_DRAWN as u64 - slot as u64, StrokeProps::TETRIS);
        }
        // all tetris: the oldest piece (highest slot index here, lowest tick) goes,
        // so the guns never dead-lock on a terrain-locked board.
        assert_eq!(claim_stroke_slot(&paths, &nodes), Some(MAX_DRAWN - 1));
    }

    #[test]
    fn baked_drawing_and_traveling_strokes_are_untouchable() {
        let nodes = zero_pool();
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];
        for (slot, path) in paths.iter_mut().enumerate() {
            *path = settled_stroke(slot as u64, StrokeProps::PEN);
        }
        paths[0].owner = -1; // baked stage stroke
        paths[1].drawing = true; // mid-draw
        paths[2].vel = Vector2::new(60.0, 0.0); // traveling body
        assert_eq!(
            claim_stroke_slot(&paths, &nodes),
            Some(3),
            "oldest EVICTABLE stroke, skipping baked/drawing/traveling"
        );
    }
}
