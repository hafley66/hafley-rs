//! Same Falcon input tape, selected buffering policy, real SQL presentation and GGRS replay.
use crate::fixture::{self, baseline, sql_viewer};
use falcon_simulation::{Action, InputBuffer, World};
use rollback::RollbackSim;
use serde::Serialize;
use sql_viewer::boundary::{
    Boundary, Row,
    contracts::{BUFFER_TICKS, BufferInspection, BufferPolicy, CONTROL_ACTION_LABELS},
};
use std::sync::Arc;
type Error = Box<dyn std::error::Error>;

#[derive(Clone)]
struct Config {
    actions: Arc<[Action]>,
    policy: BufferPolicy,
}
struct Sim;
impl RollbackSim for Sim {
    type State = World;
    type Input = u8;
    type Config = Config;
    fn initial(cx: &Config) -> World {
        World {
            input_buffer: Some(InputBuffer {
                window: cx.policy.window_frames,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn advance(state: &World, inputs: &[u8], cx: &Config) -> World {
        let mut next = state.clone();
        next.input_buffer.as_mut().unwrap().cancel = inputs[0] & 4 != 0;
        falcon_simulation::advance_world(&mut next, inputs[0] & 3, &cx.actions);
        next
    }
    fn checksum(state: &World) -> u128 {
        serde_json::to_vec(state)
            .unwrap()
            .into_iter()
            .fold(0u128, |hash, byte| {
                hash.wrapping_mul(257).wrapping_add(u128::from(byte) + 1)
            })
    }
}
fn input(tick: u32) -> u8 {
    match tick {
        20 | 100 => 1,
        21 | 101 => 2,
        102 => 4,
        _ => 0,
    }
}
fn inspect(world: &World) -> BufferInspection {
    let b = world.input_buffer.as_ref().unwrap();
    BufferInspection {
        tick: i64::from(world.frame - 1),
        pending: b.state.remaining().is_some(),
        remaining_frames: b.state.remaining().unwrap_or(0),
        consumed: b.consumed,
        expired: b.expired,
        cancelled: b.cancelled,
    }
}
#[derive(Serialize)]
struct Receipt {
    policy: BufferPolicy,
    states: Vec<World>,
    inspection: Vec<BufferInspection>,
    restore_requests: Vec<Vec<i32>>,
    advances: Vec<usize>,
    serialized_replayed: usize,
    sql_frames: usize,
}

fn verify(
    actions: &[brawllib_rs::high_level_fighter::HighLevelSubaction],
    window: u32,
) -> Result<(Receipt, Vec<Vec<Row>>), Error> {
    let cx = Config {
        actions: fixture::bake(actions).into(),
        policy: BufferPolicy {
            window_frames: window,
        },
    };
    let tape: Vec<_> = (0..BUFFER_TICKS).map(|tick| vec![input(tick)]).collect();
    let trace = rollback::proof::run::<Sim>(&cx, &tape, 7)?;
    let states = trace.states;
    let mut all_rows = Vec::new();
    let mut boundary = Boundary::new()?;
    for (tick, state) in states.iter().enumerate() {
        let rows = sql_viewer::encode(state, actions, false, tape[tick][0] & 3);
        assert!(boundary.publish(std::slice::from_ref(&rows)));
        let (_, sql) = sql_viewer::boundary::read_frame(&boundary.reader()?, tick as i64)?;
        assert_eq!(rows, sql);
        assert_eq!(sql_viewer::draw(&rows), sql_viewer::draw(&sql));
        all_rows.push(sql);
    }
    let inspection: Vec<_> = states.iter().map(inspect).collect();
    let consumes: Vec<_> = inspection
        .iter()
        .filter(|s| s.consumed)
        .map(|s| s.tick)
        .collect();
    let first_air = 20 + actions[3].frames.len() as i64 + 1;
    assert_eq!(consumes, if window == 0 { vec![] } else { vec![first_air] });
    assert!(inspection[21].pending);
    assert_eq!(inspection[22].expired, window == 0);
    assert!(inspection[102].cancelled);
    assert!(inspection[102..].iter().all(|s| !s.consumed));
    let checkpoints = [21usize, 22, first_air as usize, 101, 102];
    let replayed = rollback::proof::restore_suffixes::<Sim, Error>(
        &cx,
        &tape,
        &states,
        &checkpoints,
        |state| Ok(state.clone()),
    )?;
    let json = rollback::proof::restore_suffixes::<Sim, Error>(
        &cx,
        &tape,
        &states,
        &checkpoints,
        |state| Ok(serde_json::from_slice(&serde_json::to_vec(state)?)?),
    )?;
    let binary = rollback::proof::restore_suffixes::<Sim, Error>(
        &cx,
        &tape,
        &states,
        &checkpoints,
        |state| {
            let bytes = bincode::serde::encode_to_vec(state, bincode::config::standard())?;
            let (decoded, used) =
                bincode::serde::decode_from_slice(&bytes, bincode::config::standard())?;
            assert_eq!(used, bytes.len());
            Ok(decoded)
        },
    )?;
    assert_eq!((json, binary), (replayed, replayed));
    Ok((
        Receipt {
            policy: cx.policy,
            states,
            inspection,
            restore_requests: trace.restore_requests,
            advances: trace.advances,
            serialized_replayed: replayed,
            sql_frames: all_rows.len(),
        },
        all_rows,
    ))
}

pub fn run(record: bool) -> Result<(), Error> {
    let actions = baseline::load_controlled()?;
    let proofs = [verify(&actions, 0)?, verify(&actions, 8)?];
    std::fs::write(
        "buffer-proof.json",
        serde_json::to_vec_pretty(&[&proofs[0].0, &proofs[1].0])?,
    )?;
    if !record {
        return Ok(());
    }
    let mut capture = game_capture::Capture::new("buffer-proof.mp4")?;
    let labels: Vec<_> = CONTROL_ACTION_LABELS.split('|').collect();
    for tick in 0..BUFFER_TICKS as usize {
        let mut vertices = Vec::new();
        for (panel, (proof, rows)) in proofs.iter().enumerate() {
            let mut mesh = sql_viewer::draw(&rows[tick]);
            for v in &mut mesh {
                v[0] = (v[0] + 1.0) * 0.5 - 1.0 + panel as f32;
            }
            vertices.extend(mesh);
            let world = &proof.states[tick];
            let status = &proof.inspection[tick];
            let event = if status.cancelled {
                "CANCELLED"
            } else if status.consumed {
                "CONSUMED"
            } else if status.expired {
                "EXPIRED"
            } else if status.pending {
                "PENDING"
            } else {
                "EMPTY"
            };
            let lines = [
                format!("LAB BUFFER {} FRAMES", proof.policy.window_frames),
                format!(
                    "TICK {tick:03} / {}",
                    match input(tick as u32) {
                        1 => "JUMP",
                        2 => "ATTACK",
                        4 => "CANCEL",
                        _ => "NO INPUT",
                    }
                ),
                format!(
                    "{} / POSE {}",
                    labels[world.view.action],
                    world.view.frame + 1
                ),
                format!("{event} / REMAIN {}", status.remaining_frames),
                format!("GGRS LOAD {:?}", proof.restore_requests[tick]),
                format!("ADVANCES {} / SQL EXACT", proof.advances[tick]),
                proof.inspection[..=tick]
                    .iter()
                    .rev()
                    .find_map(|s| {
                        let event = if s.consumed {
                            "CONSUMED"
                        } else if s.expired {
                            "EXPIRED"
                        } else if s.cancelled {
                            "CANCELLED"
                        } else {
                            return None;
                        };
                        Some(format!("LAST {event} AT {}", s.tick))
                    })
                    .unwrap_or_else(|| "NO BUFFER EVENT YET".into()),
            ];
            for (line, label) in lines.iter().enumerate() {
                baseline::text(
                    &mut vertices,
                    label,
                    12.0 + panel as f32 * 480.0,
                    18.0 + line as f32 * 22.0,
                    1.3,
                    if status.consumed {
                        [0.3, 1.0, 0.6, 1.0]
                    } else {
                        [0.85, 0.9, 1.0, 1.0]
                    },
                );
            }
        }
        baseline::text(
            &mut vertices,
            "SAME INPUT / 3X SLOW PLAYBACK / LAB RULES",
            100.0,
            494.0,
            1.5,
            [1.0, 0.8, 0.3, 1.0],
        );
        for _ in 0..3 {
            capture.frame_checked(&vertices, |data| {
                for panel in 0..2 {
                    let colored = data
                        .chunks_exact(4)
                        .enumerate()
                        .filter(|(i, p)| {
                            let x = i % 960;
                            let y = i / 960;
                            x / 480 == panel
                                && (160..480).contains(&y)
                                && p[0] > 120
                                && p[1] < 160
                                && p[2] > 190
                        })
                        .count();
                    assert!(colored > 30, "fighter wireframe absent in panel {panel}");
                }
            })?;
        }
    }
    capture.finish()
}

#[test]
fn input_policy_sql_ggrs_and_serialized_restore() {
    let unbuffered = World::default();
    let binary = bincode::serde::encode_to_vec(&unbuffered, bincode::config::standard()).unwrap();
    let (decoded, used): (World, usize) =
        bincode::serde::decode_from_slice(&binary, bincode::config::standard()).unwrap();
    assert_eq!((decoded, used), (unbuffered, binary.len()));
    let actions = baseline::load_controlled().unwrap();
    for window in [0, 8] {
        verify(&actions, window).unwrap();
    }
}
