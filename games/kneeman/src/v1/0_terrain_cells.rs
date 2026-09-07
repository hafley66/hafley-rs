//! Kneeman's authored four-cell terrain lifecycle. Motion and collision stay with the
//! existing ink and item passes; this module owns spawning, durability, and conversion.

use super::{DT, InkPath, Item, ItemKind, SimState, StrokeProps, TETRIS_CELL, Vector2};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainCell {
    /// 1 + birth_tick * 16 + player * 4 + cell. Nonzero lets Option avoid a separate tag.
    pub id: NonZeroU64,
    pub durability: f32,
    pub broken_at: Option<NonZeroU64>,
}

/// Authored cell centers for I, O, T, L, S, in cell units. These are content coordinates.
const CELLS: [[(f32, f32); 4]; 5] = [
    [(-1.5, 0.0), (-0.5, 0.0), (0.5, 0.0), (1.5, 0.0)],
    [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)],
    [(-1.0, -0.5), (0.0, -0.5), (1.0, -0.5), (0.0, 0.5)],
    [(-0.5, -1.0), (-0.5, 0.0), (-0.5, 1.0), (0.5, 1.0)],
    [(0.0, -0.5), (1.0, -0.5), (-1.0, 0.5), (0.0, 0.5)],
];

/// Recorded playground actions: break, pick up, then throw into the neighboring cell.
pub fn playground_inputs() -> impl Iterator<Item = super::InputFrame> {
    (0..240).map(|tick| super::net::decode(super::net::encode(&super::InputFrame {
        attack: tick == 90 || tick == 140,
        grab: tick == 162,
        dir: if tick == 162 { 1.0 } else { 0.0 },
        ..super::InputFrame::default()
    })))
}

/// Shared offline/debugger fixture. The normal stage and ship remain; four cells and a
/// dropper exercise terrain destruction through ordinary fighter and item inputs.
pub fn playground() -> SimState {
    let mut state = SimState::spawn();
    state.tick = 1;
    state.fighters[0].char_id = 2;
    state.fighters[1].char_id = 3;
    state.fighters[0].pos = Vector2::new(420.0, super::GROUND_Y);
    state.fighters[0].state = super::CharState::Stand;
    state.fighters[0].ground_plat = 0;
    assert!(spawn_cells(
        &mut state,
        0,
        Vector2::new(550.0, 650.0),
        Vector2::ZERO,
        StrokeProps::TETRIS,
        0,
        10.0
    ));
    super::spawn_kind(
        &mut state,
        ItemKind::TetrisDropper,
        super::ToolKind::TrailPen,
        super::StrokeRegistry::TETRIS_ROW,
        &super::Tune::default(),
    );
    state.items[1].pos = Vector2::new(300.0, 700.0);
    state
}

/// All-or-nothing allocation: only unused paths are eligible; a full board keeps ammo.
/// Each cell owns five arena nodes, one path, and its rollback-resident identity.
pub fn spawn_cells(
    state: &mut SimState,
    shape: u8,
    at: Vector2,
    velocity: Vector2,
    props: StrokeProps,
    maker: i8,
    durability: f32,
) -> bool {
    if !durability.is_finite() || durability <= 0.0 {
        return false;
    }
    if !(0..super::MAX_PLAYERS as i8).contains(&maker) {
        return false;
    }
    let Some(base_id) = state
        .tick
        .checked_mul(16)
        .and_then(|tick| tick.checked_add(maker as u64 * 4 + 1))
    else {
        return false;
    };
    let Some(last_id) = base_id.checked_add(3) else {
        return false;
    };
    if state
        .paths
        .iter()
        .filter_map(|p| p.cell)
        .chain(state.items.iter().filter_map(|i| i.cell))
        .any(|cell| (base_id..=last_id).contains(&cell.id.get()))
    {
        return false;
    }
    let slots: Vec<_> = state
        .paths
        .iter()
        .enumerate()
        .filter(|(_, path)| !path.active())
        .take(4)
        .map(|(i, _)| i)
        .collect();
    if slots.len() != 4 {
        return false;
    }
    // Reserve on a copy so arena exhaustion cannot partially consume a shot.
    let mut free = state.free;
    let mut spans = [0; 4];
    for span in &mut spans {
        let Some(start) = free.alloc(5) else {
            return false;
        };
        *span = start;
    }
    state.free = free;
    for (index, (&slot, &(x, y))) in slots.iter().zip(&CELLS[shape as usize % 5]).enumerate() {
        let center = at + Vector2::new(x, y) * TETRIS_CELL;
        let half = TETRIS_CELL * 0.5;
        let points = [
            Vector2::new(-half, -half),
            Vector2::new(half, -half),
            Vector2::new(half, half),
            Vector2::new(-half, half),
            Vector2::new(-half, -half),
        ];
        let mut path = InkPath::EMPTY;
        path.start = spans[index] as u16;
        path.len = 5;
        path.pos = center;
        path.owner = maker;
        path.stroke_born = state.tick;
        path.props = props;
        path.vel = velocity;
        path.mass = 4.0 * TETRIS_CELL * props.density;
        path.cell = Some(TerrainCell {
            id: NonZeroU64::new(base_id + index as u64).unwrap(),
            durability,
            broken_at: None,
        });
        for (offset, point) in points.into_iter().enumerate() {
            state.nodes[spans[index] as usize + offset] = super::InkNode {
                pt: point,
                ..super::InkNode::ZERO
            };
        }
        super::stage::classify(&path, &mut state.nodes);
        state.paths[slot] = path;
    }
    true
}

/// A depleted cell becomes a normal hand item. Its identity and exact break tick follow it.
/// Item capacity exhaustion leaves the depleted cell pending until a slot is available.
pub(crate) fn detach_depleted(state: &mut SimState) {
    for path_slot in 0..state.paths.len() {
        let path = state.paths[path_slot];
        let Some(mut cell) = path.cell else {
            continue;
        };
        if !path.active() || path.percent < cell.durability {
            continue;
        }
        let Some(item_slot) = state.items.iter().position(|item| !item.active()) else {
            break;
        };
        cell.broken_at = NonZeroU64::new(state.tick);
        state.items[item_slot] = Item {
            kind: ItemKind::TerrainCell,
            pos: path.pos,
            vel: path.vel / DT,
            hp: 20.0,
            cell: Some(cell),
            ..Item::EMPTY
        };
        state.paths[path_slot].release(&mut state.free);
    }
}
