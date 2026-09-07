use super::terrain_cells::{detach_depleted, spawn_cells};
use super::*;

#[test]
fn falcon_breaks_picks_up_and_throws_cells_through_recorded_inputs() {
    let initial = terrain_cells::playground();
    let tune = Tune::default();
    let mut state = initial;
    let mut tape = Vec::new();
    let mut snapshots = vec![(0, bincode::serialize(&initial).unwrap())];
    let mut held_slot = None;
    for (tick, input) in terrain_cells::playground_inputs().enumerate() {
        state = step(&state, &[&input, &InputFrame::default()], &tune);
        if tick == 159 {
            let broken: Vec<_> = state.items.iter().filter_map(|item| item.cell).collect();
            assert!(!broken.is_empty(), "ordinary Falcon attacks must detach terrain cells");
            assert!(broken.iter().all(|cell| cell.broken_at.is_some()));
            assert_eq!(broken.len() + state.paths.iter()
                .filter(|p| p.active() && p.cell.is_some()).count(), 4);
            let slot = usize::try_from(state.fighters[0].holding).unwrap();
            assert_eq!(state.items[slot].kind, ItemKind::TerrainCell);
            assert_eq!(state.items[slot].owner, 0);
            assert!(!state.items[slot].thrown);
            held_slot = Some(slot);
        }
        if tick == 162 {
            let item = state.items[held_slot.unwrap()];
            assert_eq!(state.fighters[0].holding, -1);
            assert!(item.thrown && item.vel.x > 0.0);
            assert_eq!(item.owner, 0, "throw excludes its owner from self-hits");
        }
        if tick == 163 {
            assert!(!state.items[held_slot.unwrap()].active(), "spent on contact");
            assert_eq!(state.paths.iter().filter_map(|p| p.cell.map(|c| (c.id.get(), p.percent)))
                .collect::<Vec<_>>(), vec![(18, tune.throw_item.hit.damage), (19, 0.0), (20, 0.0)]);
            assert_eq!(state.fighters[0].damage, 0.0);
        }
        tape.push((input, net::checksum(&state)));
        if [139, 161, 163].contains(&tick) {
            snapshots.push((tape.len(), bincode::serialize(&state).unwrap()));
        }
    }
    for (offset, bytes) in snapshots {
        let mut replay: SimState = bincode::deserialize(&bytes).unwrap();
        for (input, expected) in &tape[offset..] {
            replay = step(&replay, &[input, &InputFrame::default()], &tune);
            assert_eq!(net::checksum(&replay), *expected, "restore {offset}, tick {}", replay.tick);
        }
    }
}

fn armed() -> (SimState, Tune) {
    let mut state = SimState::spawn_n(1);
    state.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    state.fighters[0].state = CharState::Stand;
    state.fighters[0].ground_plat = 0;
    state.fighters[0].holding = 1;
    state.items[1] = Item {
        kind: ItemKind::TetrisDropper,
        owner: 0,
        gas: 3.0,
        gas_max: 3.0,
        stroke: StrokeRegistry::TETRIS_ROW,
        ..Item::EMPTY
    };
    (state, Tune::default())
}

#[test]
fn gun_cells_damage_detach_pickup_throw_and_snapshot_replay() {
    let (initial, tune) = armed();
    let idle = InputFrame::default();
    let fired = step(
        &initial,
        &[&InputFrame {
            attack: true,
            aim_y: -1.0,
            ..idle
        }],
        &tune,
    );
    let slots: Vec<_> = fired
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.cell.is_some())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(slots.len(), 4);
    assert_eq!(fired.items[1].gas, 2.0);
    let snapshot = bincode::serialize(&fired).unwrap();
    let mut state = fired;
    let mut hashes = Vec::new();
    for _ in 0..180 {
        state = step(&state, &[&idle], &tune);
        hashes.push(net::checksum(&state));
    }
    let mut replay: SimState = bincode::deserialize(&snapshot).unwrap();
    for expected in hashes {
        replay = step(&replay, &[&idle], &tune);
        assert_eq!(net::checksum(&replay), expected);
    }
    assert!(
        slots
            .iter()
            .all(|&slot| state.paths[slot].active() && !state.paths[slot].traveling()),
        "all four cells must settle as supporting terrain"
    );
    let slot = slots[0];
    let cell = state.paths[slot].cell.unwrap();
    let contact = (state.paths[slot].world_pt(0, &state.nodes)
        + state.paths[slot].world_pt(1, &state.nodes))
        * 0.5;
    let hit = Hitbox {
        damage: 4.0,
        bkb: 30.0,
        kbg: 20.0,
        angle: 45.0,
        ..Hitbox::NONE
    };
    // The same hit/ink contact entry used by weapons, with a small box isolated to one cell.
    stage::strike_ink(
        &mut state.paths,
        contact,
        1.0,
        &hit,
        4.0,
        1.0,
        &state.nodes,
        &tune,
    );
    detach_depleted(&mut state);
    assert_eq!(state.paths[slot].percent, 4.0);
    assert!(!state.paths[slot].traveling());
    let before_break = bincode::serialize(&state).unwrap();
    let break_once = |state: &mut SimState| {
        stage::strike_ink(
            &mut state.paths,
            contact,
            1.0,
            &hit,
            6.0,
            1.0,
            &state.nodes,
            &tune,
        );
        detach_depleted(state);
    };
    break_once(&mut state);
    let mut restored: SimState = bincode::deserialize(&before_break).unwrap();
    break_once(&mut restored);
    assert_eq!(net::checksum(&restored), net::checksum(&state));
    assert!(!state.paths[slot].active());
    assert!(
        slots[1..]
            .iter()
            .all(|&i| state.paths[i].active() && state.paths[i].percent == 0.0)
    );
    let item_slot = state
        .items
        .iter()
        .position(|item| item.kind == ItemKind::TerrainCell)
        .unwrap();
    assert_eq!(state.items[item_slot].cell.unwrap().id, cell.id);
    assert_eq!(
        state.items[item_slot]
            .cell
            .unwrap()
            .broken_at
            .unwrap()
            .get(),
        state.tick
    );
    // Position the fixture fighter within the existing item pickup reach, then use ordinary acts.
    state.fighters[0].holding = -1;
    state.items[1] = Item::EMPTY;
    state.fighters[0].pos = state.items[item_slot].pos + Vector2::new(0.0, 60.0);
    state.fighters[0].ground_plat = -1;
    state.fighters[0].ground_ink = -1;
    state.fighters[0].state = CharState::Air;
    item::pickup_item(&mut state, 0, &tune);
    assert_eq!(state.fighters[0].holding, item_slot as i8);
    item::throw_item(&mut state, 0, ThrowDir::Forward, &tune);
    assert_eq!(state.fighters[0].holding, -1);
    assert!(state.items[item_slot].thrown && state.items[item_slot].vel.x > 0.0);
    let before_throw = bincode::serialize(&state).unwrap();
    let mut replay: SimState = bincode::deserialize(&before_throw).unwrap();
    for _ in 0..60 {
        state = step(&state, &[&idle], &tune);
        replay = step(&replay, &[&idle], &tune);
        assert_eq!(net::checksum(&state), net::checksum(&replay));
    }
}

#[test]
fn cell_allocation_and_conversion_keep_state_when_capacity_is_exhausted() {
    let (mut state, tune) = armed();
    let mut full = state;
    let occupied = full.paths[SHIP_SLOT];
    full.paths.fill(occupied);
    let before = bincode::serialize(&full).unwrap();
    assert!(!spawn_cells(
        &mut full,
        0,
        Vector2::ZERO,
        Vector2::ZERO,
        StrokeProps::TETRIS,
        0,
        10.0
    ));
    assert_eq!(bincode::serialize(&full).unwrap(), before);
    state = step(
        &state,
        &[&InputFrame {
            attack: true,
            ..InputFrame::default()
        }],
        &tune,
    );
    let slot = state.paths.iter().position(|p| p.cell.is_some()).unwrap();
    state.paths[slot].percent = 10.0;
    state.items.fill(Item {
        kind: ItemKind::Pen,
        ..Item::EMPTY
    });
    let before = bincode::serialize(&state).unwrap();
    detach_depleted(&mut state);
    assert_eq!(bincode::serialize(&state).unwrap(), before);
    state.items[3] = Item::EMPTY;
    detach_depleted(&mut state);
    assert!(!state.paths[slot].active());
    assert_eq!(state.items[3].kind, ItemKind::TerrainCell);
}
