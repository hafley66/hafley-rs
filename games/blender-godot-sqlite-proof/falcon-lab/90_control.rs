//! Fixed-step player input through the existing simulation and SQLite publication.
use crate::fixture::{self, baseline, sql_viewer};
use sql_viewer::boundary::{Boundary, Row, ROW_CAPACITY};
use sql_viewer::boundary::contracts::{
    ControlInput, ControlledStatus, ControlProof, FrameQuery, RowPublisher,
    CONTROL_TICKS, CONTROL_SNAPSHOT,
};
use falcon_simulation::{Simulation, World};

type Error = Box<dyn std::error::Error>;

/// Offline source coverage, including fields not yet interpreted by the runtime.
pub fn inspect_import() -> Result<(), Error> {
    let actions = baseline::load_controlled()?;
    let rows: Vec<_> = actions.iter().map(|a| serde_json::json!({
        "name": a.name, "frames": a.frames.len(), "iasa": a.iasa,
        "landing_lag": a.landing_lag, "bad_interrupts": a.bad_interrupts,
        "interruptible": a.frames.iter().enumerate().filter_map(|(i,f)| f.interruptible.then_some(i)).collect::<Vec<_>>(),
        "landing_enabled": a.frames.iter().enumerate().filter_map(|(i,f)| f.landing_lag.then_some(i)).collect::<Vec<_>>(),
        "scripts": a.scripts,
    })).collect();
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

/// Offline decoder output and native presentation oracle for the browser build.
pub fn bake_web(path: &std::path::Path) -> Result<(), Error> {
    let actions = baseline::load_controlled()?;
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
    println!("WEB_BAKE_OK bytes={} actions={} native_ticks={CONTROL_TICKS}", bytes.len(), actions.len());
    Ok(())
}

pub fn demo_input(tick: i32) -> ControlInput {
    ControlInput {
        buttons: match tick { 60 | 180 | 270 => 1, 74 | 210 => 2, _ => 0 },
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
        let actions = baseline::load_controlled()?;
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
        assert_eq!(hit_ticks, [87]);
        let transitions: Vec<_> = recorded.iter().enumerate().filter(|(i,w)| *i == 0 || recorded[i-1].view.action != w.view.action)
            .map(|(i,w)| (i, w.view.action)).collect();
        assert_eq!(transitions, [
            (0,0), (60,3), (64,1), (74,2), (114,4), (117,6), (120,0),
            (180,3), (184,1), (210,2), (237,5), (256,0), (270,3), (274,1),
        ]);
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
    fn imported_flags_and_transition_snapshots_preserve_recovery() {
        let actions = baseline::load_controlled().unwrap();
        assert_eq!(actions.iter().map(|a| (a.name.as_str(), a.frames.len(), a.iasa, a.landing_lag, a.bad_interrupts)).collect::<Vec<_>>(), [
            ("Wait1",61,None,None,false), ("JumpF",36,None,None,false),
            ("AttackAirF",40,Some(35),Some(19.0),false), ("JumpSquat",4,None,None,false),
            ("Fall",9,None,None,false), ("LandingAirF",19,Some(19),None,false),
            ("LandingHeavy",3,Some(3),None,false),
        ]);
        assert_eq!(actions[2].frames.iter().enumerate().filter_map(|(i,f)| f.landing_lag.then_some(i)).collect::<Vec<_>>(), (6..35).collect::<Vec<_>>());
        let mut sim = Simulation::new(fixture::bake(&actions).into(), true);
        let mut snapshots = Vec::new();
        let states: Vec<_> = (0..300).map(|t| {
            if [60,64,114,117,120,210,237,256,270].contains(&t) { snapshots.push((t,sim.save())); }
            let input = demo_input(t);
            sim.advance_controlled(input.buttons as u8, input.axis).clone()
        }).collect();
        for (start,snapshot) in snapshots {
            sim.load(&snapshot);
            for t in start..300 {
                let input = demo_input(t);
                assert_eq!(sim.advance_controlled(input.buttons as u8,input.axis), &states[t as usize]);
            }
        }
        // A jump pressed during landing is rejected; holding it is not a fresh edge.
        let mut sim = Simulation::new(fixture::bake(&actions).into(), false);
        for t in 0..238 { let input=demo_input(t); sim.advance_controlled(input.buttons as u8,0.0); }
        for _ in 238..270 { assert_ne!(sim.advance_controlled(1,0.0).view.action,3); }
        sim.advance_controlled(0,0.0);
        assert_eq!(sim.advance_controlled(1,0.0).view.action,3);
    }
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
