//! V1 hull gravity receipt, compiled only by the shell's test target.
//! Authority: V1 stage::bake_ship sets gravity_scale to zero for thrust-only flight.

use crate::v1::{InputFrame, SHIP_SLOT, SimState, Tune};
use cascade::{DebugFrame, GRAVITY_SCALE, GravityScale, Rules};

fn hull(snapshot: &SimState, entity: u32) -> bool {
    entity == SHIP_SLOT as u32 && snapshot.paths[SHIP_SLOT].len > 0
}

fn active_stroke(snapshot: &SimState, entity: u32) -> bool {
    snapshot
        .paths
        .get(entity as usize)
        .is_some_and(|path| path.len > 0)
}

fn rules() -> Rules<SimState> {
    let mut rules = Rules::default();
    rules.set(GRAVITY_SCALE, GravityScale(1.0));
    rules.when(hull, (0, 2), GRAVITY_SCALE, GravityScale(0.25));
    rules.when(active_stroke, (0, 1), GRAVITY_SCALE, GravityScale(1.0));
    rules.when(hull, (0, 2), GRAVITY_SCALE, GravityScale(0.0));
    rules
}

fn step_receipt(
    previous: &SimState,
    inputs: &[&InputFrame],
    tune: &Tune,
    rules: &Rules<SimState>,
) -> (SimState, DebugFrame) {
    let debug = rules.resolve(previous, SHIP_SLOT as u32, GRAVITY_SCALE);
    let mut prepared = *previous;
    if let Some(winner) = debug.winner {
        prepared.paths[SHIP_SLOT].props.gravity_scale = winner.value.0;
    }
    (crate::v1::step(&prepared, inputs, tune), debug)
}

#[path = "0_ship.test.rs"]
mod tests;
