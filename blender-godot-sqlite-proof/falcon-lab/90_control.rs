//! Fixed-step player input through the existing simulation and SQLite publication.
use crate::fixture::{self, baseline, sql_viewer};
use sql_viewer::boundary::{Boundary, Row, ROW_CAPACITY};
use sql_viewer::boundary::contracts::{
    ControlInput, ControlledStatus, ControlProof, FrameQuery, RowPublisher,
    CONTROL_TICKS, CONTROL_SNAPSHOT,
};
use falcon_simulation::{Simulation, World};

type Error = Box<dyn std::error::Error>;

/// Offline decoder output and native presentation oracle for the browser build.
pub fn bake_web(path: &std::path::Path) -> Result<(), Error> {
    let actions = baseline::load()?;
    let baked = fixture::bake(&actions);
    let poses: Vec<Vec<Vec<Row>>> = actions.iter().enumerate().map(|(action, a)| {
        a.frames.iter().enumerate().map(|(frame, _)| {
            let mut world = World::default();
            world.frame = 1;
            world.view.action = action;
            world.view.frame = frame;
            sql_viewer::encode(&world, &actions, false, 0)
        }).collect()
    }).collect();
    let inputs: Vec<_> = (0..CONTROL_TICKS).map(|t| demo_input(t as i32)).collect();
    let mut simulation = Simulation::new(baked.clone().into(), true);
    let expected: Vec<_> = inputs.iter().map(|input| {
        let world = simulation.advance_controlled(input.buttons as u8, input.axis);
        sql_viewer::encode(world, &actions, false, input.buttons as u8)
    }).collect();
    let bytes = bincode::serde::encode_to_vec((baked, poses, inputs, expected), bincode::config::standard())?;
    std::fs::write(path, &bytes)?;
    println!("WEB_BAKE_OK bytes={} actions=3 native_ticks={CONTROL_TICKS}", bytes.len());
    Ok(())
}

pub fn demo_input(tick: i32) -> ControlInput {
    ControlInput {
        buttons: falcon_simulation::fixture_input(tick % 120).into(),
        axis: match tick {
            60..=95 | 180..=215 => 1.0,
            135..=160 => -1.0,
            _ => 0.0,
        },
    }
}

pub struct Controlled {
    actions: Vec<brawllib_rs::high_level_fighter::HighLevelSubaction>,
    simulation: Simulation,
    boundary: Boundary,
    scratch: Vec<Row>,
    pub recorded: Option<Vec<World>>,
}

impl Controlled {
    pub fn new(record: bool) -> Result<Self, Error> {
        let actions = baseline::load()?;
        let simulation = Simulation::new(fixture::bake(&actions).into(), true);
        Ok(Self {
            actions, simulation, boundary: Boundary::new()?,
            scratch: vec![Row::new(0, 0, 0); ROW_CAPACITY],
            recorded: record.then(|| Vec::with_capacity(CONTROL_TICKS as usize)),
        })
    }

    #[tracing::instrument(target = "falcon::control", level = "trace", skip_all, fields(buttons = input.buttons, axis = input.axis))]
    pub fn step(&mut self, input: ControlInput) -> Result<(Vec<Row>, ControlledStatus), Error> {
        let buttons = u8::try_from(input.buttons)?;
        assert_eq!(buttons & !3, 0, "unsupported input bits");
        let world = self.simulation.advance_controlled(buttons, input.axis);
        if let Some(recorded) = &mut self.recorded {
            assert!(recorded.len() < CONTROL_TICKS as usize);
            recorded.push(world.clone());
        }
        let rows = sql_viewer::encode(world, &self.actions, false, buttons);
        let id = RowPublisher::publish(&mut self.boundary, &rows).unwrap();
        let tick = i64::from(world.frame - 1);
        let read = FrameQuery::read_frame(&mut self.boundary, tick, &mut self.scratch).unwrap();
        assert_eq!(read.id, id);
        assert_eq!(&self.scratch[..read.rows_written as usize], rows);
        Ok((rows, ControlledStatus { simulation_tick: tick, renderer_generation: id.generation, input }))
    }

    /// Recompute every captured state, then restore before the second jump.
    pub fn verify(&self) -> ControlProof {
        let recorded = self.recorded.as_ref().unwrap();
        assert_eq!(recorded.len(), CONTROL_TICKS as usize);
        let mut sim = Simulation::new(fixture::bake(&self.actions).into(), true);
        let mut snapshot = None;
        for (tick, expected) in recorded.iter().enumerate() {
            if tick == CONTROL_SNAPSHOT as usize { snapshot = Some(sim.save()); }
            let input = demo_input(tick as i32);
            assert_eq!(sim.advance_controlled(input.buttons as u8, input.axis), expected);
        }
        sim.load(snapshot.as_ref().unwrap());
        for tick in CONTROL_SNAPSHOT..CONTROL_TICKS {
            let input = demo_input(tick as i32);
            assert_eq!(sim.advance_controlled(input.buttons as u8, input.axis), &recorded[tick as usize]);
        }
        let hit_ticks: Vec<_> = recorded.iter().filter(|w| w.view.hit.is_some()).map(|w| i64::from(w.frame - 1)).collect();
        assert_eq!(hit_ticks, [91]);
        assert!(recorded[160].view.root[2] < recorded[134].view.root[2]);
        assert!(recorded[200].view.root[1] > 0.0);
        assert_ne!(recorded[95].bag.as_ref().unwrap().position, recorded[90].bag.as_ref().unwrap().position);
        let last = recorded.last().unwrap();
        ControlProof { ticks: CONTROL_TICKS, hits: last.hit_count as u32, damage: last.damage.into(),
            replayed: CONTROL_TICKS - CONTROL_SNAPSHOT, hit_ticks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controlled_motion_launch_and_sql_replay_are_exact() {
        let mut run = Controlled::new(true).unwrap();
        for tick in 0..CONTROL_TICKS {
            run.step(demo_input(tick as i32)).unwrap();
        }
        let proof = run.verify();
        assert_eq!((proof.hits, proof.damage, proof.replayed), (1, 18.0, 120));
    }

    #[test]
    fn zero_axis_stays_put_and_opposite_inputs_diverge() {
        let mut run = Controlled::new(false).unwrap();
        let snapshot = run.simulation.save();
        for _ in 0..60 { run.simulation.advance_controlled(0, 0.0); }
        assert_eq!(run.simulation.state().view.root, [0.0, 0.0, -12.0]);
        run.simulation.load(&snapshot);
        let right = run.simulation.advance_controlled(0, 1.0).view.root[2];
        run.simulation.load(&snapshot);
        let left = run.simulation.advance_controlled(0, -1.0).view.root[2];
        assert_eq!((right, left), (-11.1, -12.9));
    }

    #[test]
    fn invalid_input_preserves_authoritative_state() {
        let mut run = Controlled::new(false).unwrap();
        let before = run.simulation.state().clone();
        for input in [
            ControlInput { buttons: 4, axis: 0.0 },
            ControlInput { buttons: 0, axis: -2.0 },
            ControlInput { buttons: 0, axis: 2.0 },
            ControlInput { buttons: 0, axis: f32::NAN },
            ControlInput { buttons: 0, axis: f32::INFINITY },
        ] {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.step(input))).is_err());
            assert_eq!(run.simulation.state(), &before);
        }
    }
}
