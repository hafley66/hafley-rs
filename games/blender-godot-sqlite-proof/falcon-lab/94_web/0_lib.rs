//! Browser host. Decoder/recording tools run offline; simulation and SQL stay shared.
#![allow(dead_code)]
use godot::prelude::*;
use smash::fighters::falcon;
use falcon::{Action, Simulation, Snapshot, World};

#[path = "../1b_boundary.rs"]
pub mod boundary;
#[path = "../0a_live_values.rs"]
mod live_values;
#[path = "../27_geometry.rs"]
mod geometry;
// Existing generated adapters reference the lab's canonical contract path.
pub mod fixture { pub mod sql_viewer { pub use crate::boundary; } }
#[path = "../contracts/2_godot_auto.rs"]
mod payload;
use boundary::contracts::*;
use payload::{GodotFramePayload, GodotMeshReceipt};

type Assets = (Vec<Action>, Vec<Vec<Vec<Row>>>, Vec<ControlInput>, Vec<Vec<Row>>);

struct Runtime {
    simulation: Simulation,
    poses: Vec<Vec<Vec<Row>>>,
    inputs: Vec<ControlInput>,
    expected: Vec<Vec<Row>>,
    boundary: boundary::Boundary,
    output: Vec<Row>,
    recorded: Vec<World>,
    snapshot: Option<Snapshot>,
    demo: bool,
}

struct Extension;
#[gdextension]
unsafe impl ExtensionLibrary for Extension {}

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct FalconSql {
    base: Base<RefCounted>,
    runtime: Option<Runtime>,
    pending: Option<GodotFramePayload>,
    receipts: usize,
}

#[godot_api]
impl FalconSql {
    #[func]
    fn proof_version(&self) -> GString { "falcon-sql-gdext-1".into() }

    #[func]
    fn start_controlled(&mut self, record: bool) {
        let bytes = include_bytes!("1_assets.bin");
        let ((actions, poses, inputs, expected), used): (Assets, usize) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard()).unwrap();
        assert_eq!(used, bytes.len());
        self.runtime = Some(Runtime {
            simulation: Simulation::new(actions.into(), true), poses, inputs, expected,
            boundary: boundary::Boundary::new().unwrap(),
            output: vec![Row::new(0, 0, 0); ROW_CAPACITY as usize],
            recorded: Vec::with_capacity(if record { CONTROL_TICKS as usize } else { 0 }),
            snapshot: None, demo: record,
        });
        self.receipts = 0;
        self.pending = None;
    }

    #[func]
    fn control_ticks(&self) -> i64 { CONTROL_TICKS.into() }

    #[func]
    fn control_demo_input(&self, tick: i64) -> VarDictionary {
        self.runtime.as_ref().unwrap().inputs[tick as usize].to_dictionary()
    }

    #[func]
    #[tracing::instrument(target = "falcon::web", level = "trace", skip_all)]
    fn advance_controlled(&mut self, input: VarDictionary) -> VarDictionary {
        assert!(self.pending.is_none());
        let input = ControlInput::from_dictionary(&input);
        let run = self.runtime.as_mut().unwrap();
        if run.demo && run.simulation.state().frame == CONTROL_SNAPSHOT as i32 {
            run.snapshot = Some(run.simulation.save());
        }
        let world = run.simulation.advance_controlled(input.buttons as u8, input.axis);
        let mut rows = run.poses[world.view.action][world.view.frame].clone();
        let meta = FrameValues::from_row(&rows[0]).unwrap();
        for row in &mut rows { row.tick = i64::from(world.frame - 1); }
        rows[..2].copy_from_slice(&live_values::state(world, meta.animation_x, meta.animation_y, false, input.buttons as u8));
        if run.demo {
            assert_eq!(rows, run.expected[(world.frame - 1) as usize], "native/browser presentation differs");
            run.recorded.push(world.clone());
        }
        let id = RowPublisher::publish(&mut run.boundary, &rows).unwrap();
        let read = FrameQuery::read_frame(&mut run.boundary, rows[0].tick, &mut run.output).unwrap();
        assert_eq!(read.id, id);
        assert_eq!(&run.output[..read.rows_written as usize], rows);
        let lines = geometry::wire(&rows);
        let frame = GodotFramePayload {
            rows: PackedFloat64Array::from(pack_rows(&rows).as_slice()),
            vertices: lines.iter().flat_map(|l| [l.a, l.b]).map(|p| Vector3::new(p[2], p[1], -p[0])).collect(),
            colors: lines.iter().flat_map(|l| [l.color; 2]).map(|c| Color::from_rgba(c[0], c[1], c[2], c[3])).collect(),
            status: FrameStatus::Controlled(ControlledStatus {
                simulation_tick: rows[0].tick, renderer_generation: id.generation, input,
            }),
        };
        let wire = frame.to_dictionary();
        self.pending = Some(frame);
        wire
    }

    #[func]
    fn acknowledge(&mut self, receipt: VarDictionary) -> bool {
        let receipt = GodotMeshReceipt::from_dictionary(&receipt);
        let frame = self.pending.take().unwrap();
        assert_eq!(receipt.generation as u64, frame.status.renderer_generation());
        assert_eq!(receipt.rows, frame.rows);
        assert_eq!(receipt.vertices, frame.vertices);
        self.receipts += 1;
        true
    }

    #[func]
    fn finish_controlled(&mut self, _path: GString) -> bool {
        assert!(self.pending.is_none());
        assert_eq!(self.receipts, CONTROL_TICKS as usize);
        let run = self.runtime.as_mut().unwrap();
        run.simulation.load(run.snapshot.as_ref().unwrap());
        for tick in CONTROL_SNAPSHOT..CONTROL_TICKS {
            let input = run.inputs[tick as usize];
            assert_eq!(run.simulation.advance_controlled(input.buttons as u8, input.axis), &run.recorded[tick as usize]);
        }
        let last = run.recorded.last().unwrap();
        assert_eq!((last.hit_count, last.damage), (1, 18.0));
        godot_print!("CONTROL_OK ticks=300 hits=1 damage=18 replayed=120 rows_and_mesh=exact native_rows=exact");
        true
    }
}
