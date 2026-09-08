// Reuse the preceding fixture's decoder and wireframe presentation unchanged.
#[allow(dead_code)]
#[path = "2_main.rs"]
pub(crate) mod baseline;
use falcon_simulation::sandbag;
#[path = "21_sql_viewer.rs"]
pub(crate) mod sql_viewer;
use baseline::{gpu, text};
use brawllib_rs::high_level_fighter::{CollisionBoxValues, HighLevelSubaction};
use ggrs::{
    Config, GgrsRequest, InputStatus, Message, NonBlockingSocket, PlayerType, PredictRepeatLast,
    SessionBuilder, SessionState,
};
use serde::Serialize;
use std::sync::{Arc, Mutex};

type Error = Box<dyn std::error::Error>;
const WHITE: [f32; 4] = [0.85, 0.91, 0.98, 1.0];
const CYAN: [f32; 4] = [0.25, 0.85, 0.9, 1.0];
const ORANGE: [f32; 4] = [1.0, 0.42, 0.18, 1.0];
const JUMP: u8 = 1;
const ATTACK: u8 = 2;

use falcon_simulation::World;
pub(crate) fn bake(actions: &[HighLevelSubaction]) -> Vec<falcon_simulation::Action> {
    actions
        .iter()
        .map(|a| falcon_simulation::Action {
            frames: a
                .frames
                .iter()
                .map(|f| falcon_simulation::Frame {
                    x_pos: f.x_pos,
                    y_pos: f.y_pos,
                    hit_boxes: f
                        .hit_boxes
                        .iter()
                        .filter_map(|h| {
                            let CollisionBoxValues::Hit(v) = &h.next_values else {
                                return None;
                            };
                            Some(falcon_simulation::Attack {
                                id: h.hitbox_id,
                                position: [h.next_pos.x, h.next_pos.y, h.next_pos.z],
                                radius: h.next_size,
                                enabled: v.enabled,
                                aerial: v.aerial,
                                damage: v.damage,
                                kbg: v.kbg as u32,
                                bkb: v.bkb as u32,
                                wdsk: v.wdsk as u32,
                                trajectory: v.trajectory as f32,
                            })
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}
fn input(tick: i32) -> u8 {
    falcon_simulation::fixture_input(tick)
}
fn step(world: &mut World, bits: u8, actions: &[HighLevelSubaction]) {
    falcon_simulation::advance_world(world, bits, &bake(actions));
}

struct Game;
impl Config for Game {
    type Input = u8;
    type InputPredictor = PredictRepeatLast;
    type State = World;
    type Address = usize;
}
#[derive(Default)]
struct Bus {
    tick: usize,
    held: bool,
    packets: Vec<(usize, usize, usize, Message)>,
}
struct Socket {
    id: usize,
    bus: Arc<Mutex<Bus>>,
}
impl NonBlockingSocket<usize> for Socket {
    fn send_to(&mut self, msg: &Message, addr: &usize) {
        let mut bus = self.bus.lock().unwrap();
        let due = if bus.held && self.id == 0 && (78..97).contains(&bus.tick) {
            97
        } else {
            bus.tick
        };
        bus.packets.push((due, self.id, *addr, msg.clone()));
    }
    fn receive_all_messages(&mut self) -> Vec<(usize, Message)> {
        let mut bus = self.bus.lock().unwrap();
        let tick = bus.tick;
        let mut incoming = Vec::new();
        bus.packets.retain(|(due, src, dst, msg)| {
            if *dst == self.id && *due <= tick {
                incoming.push((*src, msg.clone()));
                false
            } else {
                true
            }
        });
        incoming
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Display {
    pub world: World,
    pub applied: u8,
    pub predicted: bool,
    pub confirmed: i32,
    pub restored: Vec<i32>,
    pub saved: Vec<i32>,
    pub advances: usize,
    pub total_loads: usize,
    pub presented: Vec<Vec<sql_viewer::boundary::Row>>,
}
#[tracing::instrument(target = "falcon::rollback", level = "trace", skip_all, fields(tick = world.frame))]
fn checksum(world: &World) -> u128 {
    bincode::serde::encode_to_vec(world, bincode::config::standard())
        .unwrap()
        .iter()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        }) as u128
}
fn handle(
    world: &mut World,
    requests: Vec<GgrsRequest<Game>>,
    actions: &[HighLevelSubaction],
    loads: &mut usize,
    baked: &[falcon_simulation::Action],
) -> Display {
    let mut restored = Vec::new();
    let mut saved = Vec::new();
    let mut advances = 0;
    let mut predicted = false;
    let mut applied = 0;
    let mut presented = Vec::new();
    for request in requests {
        match request {
            GgrsRequest::SaveGameState { cell, frame } => {
                let _span = tracing::trace_span!(target: "falcon::rollback", "save", tick = frame)
                    .entered();
                assert_eq!(frame, world.frame);
                cell.save(frame, Some(world.clone()), Some(checksum(world)));
                saved.push(frame);
            }
            GgrsRequest::LoadGameState { cell, frame } => {
                let _span =
                    tracing::debug_span!(target: "falcon::rollback", "restore", tick = frame)
                        .entered();
                *world = cell.load().unwrap();
                assert_eq!(world.frame, frame);
                restored.push(frame);
                *loads += 1;
            }
            GgrsRequest::AdvanceFrame { inputs } => {
                applied = inputs[0].0;
                predicted = inputs[0].1 == InputStatus::Predicted;
                falcon_simulation::advance_world(world, applied, baked);
                presented.push(sql_viewer::encode(world, actions, predicted, applied));
                advances += 1;
            }
        }
    }
    Display {
        world: world.clone(),
        applied,
        predicted,
        confirmed: -1,
        restored,
        saved,
        advances,
        total_loads: *loads,
        presented,
    }
}

pub struct Runtime<'a> {
    actions: &'a [HighLevelSubaction],
    baked: Vec<falcon_simulation::Action>,
    bus: Arc<Mutex<Bus>>,
    peers: Vec<ggrs::P2PSession<Game>>,
    worlds: [World; 2],
    loads: [usize; 2],
    tick: usize,
}
impl<'a> Runtime<'a> {
    pub fn new(actions: &'a [HighLevelSubaction], held: bool, launch: bool) -> Result<Self, Error> {
        let bus = Arc::new(Mutex::new(Bus {
            held,
            ..Default::default()
        }));
        let mut peers = Vec::new();
        for id in 0..2 {
            let mut builder = SessionBuilder::<Game>::new()
                .with_num_players(2)?
                .with_max_prediction_window(32);
            for player in 0..2 {
                builder = builder.add_player(
                    if player == id {
                        PlayerType::Local
                    } else {
                        PlayerType::Remote(player)
                    },
                    player,
                )?;
            }
            peers.push(builder.start_p2p_session(Socket {
                id,
                bus: bus.clone(),
            })?);
        }
        for _ in 0..1000 {
            for peer in &mut peers {
                peer.poll_remote_clients();
            }
            if peers
                .iter()
                .all(|p| p.current_state() == SessionState::Running)
            {
                break;
            }
        }
        assert!(
            peers
                .iter()
                .all(|p| p.current_state() == SessionState::Running)
        );
        let mut worlds = [World::default(), World::default()];
        if launch {
            for world in &mut worlds {
                world.bag = Some(sandbag::Sandbag::default());
            }
        }

        Ok(Self {
            actions,
            baked: bake(actions),
            bus,
            peers,
            worlds,
            loads: [0, 0],
            tick: 0,
        })
    }
    pub fn tick(&self) -> usize {
        self.tick
    }
    #[tracing::instrument(target = "falcon::runtime", level = "debug", skip_all, fields(tick = self.tick, input = bits))]
    pub fn advance(&mut self, bits: u8) -> Result<[Display; 2], Error> {
        let tick = self.tick;
        self.bus.lock().unwrap().tick = tick;
        for peer in &mut self.peers {
            peer.poll_remote_clients();
        }
        let mut displays = Vec::new();
        for id in 0..2 {
            let _span =
                tracing::debug_span!(target: "falcon::rollback", "peer", peer = id).entered();
            assert_eq!(self.peers[id].current_frame(), tick as i32);
            self.peers[id].add_local_input(id, if id == 0 { bits } else { 0 })?;
            let requests = self.peers[id].advance_frame()?;
            let mut peer_display = handle(
                &mut self.worlds[id],
                requests,
                self.actions,
                &mut self.loads[id],
                &self.baked,
            );
            peer_display.confirmed = self.peers[id].confirmed_frame();
            tracing::debug!(target: "falcon::rollback", advances = peer_display.advances, restores = peer_display.restored.len(), saves = peer_display.saved.len(), confirmed = peer_display.confirmed, "requests_executed");
            for frame in &mut peer_display.presented {
                frame[0].values[15] = peer_display.restored.first().copied().unwrap_or(-1) as f64;
                frame[0].values[16] = peer_display.advances as f64;
                frame[0].values[17] = peer_display.total_loads as f64;
                frame[0].values[18] = peer_display.confirmed as f64;
            }
            displays.push(peer_display);
        }

        self.tick += 1;
        Ok(displays.try_into().unwrap())
    }
}
fn run(
    actions: &[HighLevelSubaction],
    held: bool,
    launch: bool,
) -> Result<Vec<[Display; 2]>, Error> {
    let mut runtime = Runtime::new(actions, held, launch)?;
    (0..180).map(|tick| runtime.advance(input(tick))).collect()
}

fn verify(actions: &[HighLevelSubaction], trace: &[[Display; 2]]) -> Result<(), Error> {
    let launch = trace[0][0].world.bag.is_some();
    let mut reference = World::default();
    if launch {
        reference.bag = Some(sandbag::Sandbag::default());
    }
    for (tick, pair) in trace.iter().enumerate() {
        step(&mut reference, input(tick as i32), actions);
        assert_eq!(
            pair[0].world, reference,
            "on-time peer must match direct input simulation"
        );
        if tick >= 97 {
            assert_eq!(
                pair[1].world, reference,
                "corrected peer mismatch at {tick}"
            );
        }
    }
    assert_eq!(trace[91][0].world.damage, 18.0);
    assert_eq!(trace[91][1].world.damage, 0.0);
    assert!(trace[90][1].predicted);
    assert!(!trace[97][1].restored.is_empty());
    assert!(trace[97][1].advances > 1);
    for peer in &trace[179] {
        assert_eq!(peer.world.damage, 18.0);
        assert_eq!(peer.world.hit_count, 1);
    }
    let clean = run(actions, false, launch)?;
    assert!(clean.iter().all(|pair| pair[0].world == pair[1].world));
    let repeated = run(actions, true, launch)?;
    assert!(
        trace
            .iter()
            .zip(repeated)
            .all(|(a, b)| a[0].world == b[0].world && a[1].world == b[1].world)
    );
    eprintln!(
        "ROLLBACK_OK restored={:?} advances={} final_damage=18 hit_count=1 equal_from_tick=97",
        trace[97][1].restored, trace[97][1].advances
    );
    if launch {
        let phases: Vec<_> = trace
            .iter()
            .map(|pair| pair[0].world.bag.as_ref().unwrap().phase)
            .collect();
        for phase in [
            sandbag::Phase::Hovering,
            sandbag::Phase::Hit,
            sandbag::Phase::Hitstun,
            sandbag::Phase::Falling,
            sandbag::Phase::Landed,
        ] {
            assert!(phases.contains(&phase), "missing {phase:?}");
        }
        assert_eq!(phases[91], sandbag::Phase::Hit);
        assert_eq!(phases[179], sandbag::Phase::Landed);
        assert_eq!(trace[179][0].world.bag.as_ref().unwrap().stun, 0);
        assert_eq!(trace[91][0].world.bag.as_ref().unwrap().stun, 26);
        let landed = phases
            .iter()
            .position(|p| *p == sandbag::Phase::Landed)
            .unwrap();
        let falling = phases
            .iter()
            .position(|p| *p == sandbag::Phase::Falling)
            .unwrap();
        eprintln!(
            "LAUNCH_OK kb={} hitstun=26 falling_tick={falling} landed_tick={landed} final={:?}",
            trace[91][0].world.bag.as_ref().unwrap().knockback,
            trace[179][0].world.bag
        );
    }
    Ok(())
}

fn render(actions: &[HighLevelSubaction], trace: &[[Display; 2]], id: usize) -> Result<(), Error> {
    let launch = trace[0][0].world.bag.is_some();
    let mut capture = gpu::Capture::new(&format!(
        "{}_peer{id}.mp4",
        if launch { "16" } else { "10" }
    ))?;
    for (tick, pair) in trace.iter().enumerate() {
        let d = &pair[id];
        let s = &d.world.view;
        let mut vertices = baseline::draw(
            &actions[s.action].frames[s.frame],
            s,
            tick,
            false,
            d.world.last_hit.map(|t| t as usize),
            false,
            d.world
                .bag
                .as_ref()
                .map_or([0.0, 24.0, 28.0], |bag| bag.position),
        );
        if launch {
            for v in &mut vertices {
                let x = (v[0] + 1.0) * 480.0;
                let y = (1.0 - v[1]) * 270.0;
                v[0] = (180.0 + (x - 350.0) * 0.65) / 480.0 - 1.0;
                v[1] = 1.0 - (422.0 + (y - 444.0) * 0.65) / 270.0;
            }
        }
        text(
            &mut vertices,
            if id == 0 {
                "PEER A / ON-TIME INPUT"
            } else {
                "PEER B / DELAYED INPUT"
            },
            24.0,
            16.0,
            2.5,
            WHITE,
        );
        text(
            &mut vertices,
            &format!("TICK {:03}  CONFIRMED THROUGH {:03}", tick, d.confirmed),
            24.0,
            49.0,
            1.7,
            CYAN,
        );
        text(
            &mut vertices,
            &format!(
                "STATE {} / POSE {:02}",
                ["IDLE", "JUMP", "FAIR"][s.action],
                s.frame + 1
            ),
            24.0,
            75.0,
            1.7,
            WHITE,
        );
        text(
            &mut vertices,
            &format!(
                "INPUT {} / {}",
                match d.applied {
                    JUMP => "JUMP",
                    ATTACK => "ATTACK",
                    _ => "NONE",
                },
                if d.predicted {
                    "PREDICTED"
                } else {
                    "CONFIRMED"
                }
            ),
            490.0,
            75.0,
            1.5,
            if d.predicted { ORANGE } else { CYAN },
        );
        text(
            &mut vertices,
            &format!(
                "{} / CONTACT {}",
                if s.root[1] > 0.0 {
                    "AIRBORNE"
                } else {
                    "GROUNDED"
                },
                if s.contact { "YES" } else { "NO" }
            ),
            24.0,
            101.0,
            1.3,
            CYAN,
        );
        if let Some(bag) = &d.world.bag {
            text(
                &mut vertices,
                &format!(
                    "BAG {} / STUN {:02} / GROUND {}",
                    bag.phase.label(),
                    bag.stun,
                    bag.grounded
                ),
                24.0,
                128.0,
                1.7,
                ORANGE,
            );
            text(
                &mut vertices,
                &format!(
                    "POS {:.1},{:.1} / VEL {:.2},{:.2} U/TICK",
                    bag.position[2], bag.position[1], bag.velocity[2], bag.velocity[1]
                ),
                24.0,
                154.0,
                1.5,
                CYAN,
            );
        }
        let transport = if (78..97).contains(&tick) {
            "A->B PACKETS HELD / ATTACK MISSING"
        } else if tick == 97 {
            "QUEUED INPUT DELIVERED"
        } else {
            "TRANSPORT DELIVERING"
        };
        text(
            &mut vertices,
            transport,
            24.0,
            438.0,
            1.6,
            if (78..98).contains(&tick) {
                ORANGE
            } else {
                CYAN
            },
        );
        let rollback = if !d.restored.is_empty() {
            format!(
                "RESTORE {} / REPLAY {} / ADVANCE 1",
                d.restored[0],
                d.advances - 1
            )
        } else {
            format!(
                "ROLLBACKS {} / ADVANCES THIS STEP {}",
                d.total_loads, d.advances
            )
        };
        text(&mut vertices, &rollback, 24.0, 465.0, 1.6, WHITE);
        text(
            &mut vertices,
            &format!(
                "DAMAGE {:.0}% / HITS {} / {}",
                d.world.damage,
                d.world.hit_count,
                if pair[0].world == pair[1].world {
                    "STATES MATCH"
                } else {
                    "STATES DIFFER"
                }
            ),
            24.0,
            491.0,
            1.6,
            if pair[0].world == pair[1].world {
                CYAN
            } else {
                ORANGE
            },
        );
        text(
            &mut vertices,
            if launch {
                "0.5X + HOLDS / SCRIPTED FALCON / PM DATA + MELEE KB + RAPIER"
            } else {
                "0.5X PLAYBACK / EVENT HOLDS / SCRIPTED TRAVEL"
            },
            24.0,
            518.0,
            1.1,
            WHITE,
        );
        // Presentation-only holds preserve actual tick state, making events readable.
        let phase_change = launch
            && tick > 0
            && trace[tick][0].world.bag.as_ref().unwrap().phase
                != trace[tick - 1][0].world.bag.as_ref().unwrap().phase;
        let repeats = if [60, 78, 91, 97, 179].contains(&tick) || phase_change {
            60
        } else {
            2
        };
        for _ in 0..repeats {
            if let Some(bag) = &d.world.bag {
                let x = 180.0 + bag.position[2] * 6.5;
                let y = 422.0 - bag.position[1] * 6.5;
                capture.frame_regions(
                    &vertices,
                    [
                        [40, 400, 210, 430],
                        [
                            (x - 34.0).max(0.0) as u32,
                            (x + 34.0) as u32,
                            (y - 40.0).max(200.0) as u32,
                            (y + 40.0) as u32,
                        ],
                    ],
                )?;
            } else {
                capture.frame(&vertices)?;
            }
        }
    }
    capture.finish()
}

#[allow(dead_code)]
pub(crate) fn host_fixture(
    consume: impl FnMut(&[sql_viewer::boundary::Row], &serde_json::Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let actions = baseline::load()?;
    let trace = run(&actions, true, true)?;
    verify(&actions, &trace)?;
    sql_viewer::execute_with(&trace, false, consume)
}

#[cfg(feature = "gdext")]
pub(crate) fn incremental_host(
    mut request: impl FnMut() -> Result<u8, Error>,
    consume: impl FnMut(&[sql_viewer::boundary::Row], &serde_json::Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let actions = baseline::load()?;
    let mut runtime = Runtime::new(&actions, true, true)?;
    assert_eq!(runtime.tick(), 0);
    sql_viewer::execute_stream(
        || {
            let before = runtime.tick();
            let bits = request()?;
            assert_eq!(
                runtime.tick(),
                before,
                "waiting for input advanced simulation"
            );
            let pair = runtime.advance(bits)?;
            assert_eq!(runtime.tick(), before + 1);
            Ok(pair)
        },
        false,
        consume,
        concat!(env!("CARGO_MANIFEST_DIR"), "/39_incremental_sql.json"),
    )?;
    assert_eq!(runtime.tick(), 180);
    eprintln!("INCREMENTAL_OK requested_ticks=180 executed_ticks=180 precomputed_ticks=0");
    Ok(())
}

pub fn run_cli() -> Result<(), Error> {
    baseline::telemetry::init();
    let actions = baseline::load()?;
    let sql = std::env::args().any(|arg| arg == "--sql");
    let launch = sql || std::env::args().any(|arg| arg == "--launch");
    let trace = run(&actions, true, launch)?;
    verify(&actions, &trace)?;
    if sql {
        return sql_viewer::execute(&trace, !std::env::args().any(|arg| arg == "--verify-only"));
    }
    std::fs::write(
        if launch {
            "17_launch_trace.json"
        } else {
            "11_rollback_trace.json"
        },
        serde_json::to_vec_pretty(&trace)?,
    )?;
    if std::env::args().any(|arg| arg == "--verify-only") {
        return Ok(());
    }
    render(&actions, &trace, 0)?;
    render(&actions, &trace, 1)
}
#[cfg(test)]
mod tests {
    #[test]
    fn delayed_attack_rolls_back_and_converges() {
        let actions = super::baseline::load().unwrap();
        let trace = super::run(&actions, true, false).unwrap();
        super::verify(&actions, &trace).unwrap();
    }
    #[test]
    fn launched_sandbag_physics_and_timers_converge() {
        let actions = super::baseline::load().unwrap();
        let trace = super::run(&actions, true, true).unwrap();
        super::verify(&actions, &trace).unwrap();
    }

    #[test]
    fn moving_and_ground_contact_snapshots_replay_exactly() {
        let actions = super::baseline::load().unwrap();
        for checkpoint in [105, 128] {
            let mut world = super::World {
                bag: Some(super::sandbag::Sandbag::default()),
                ..Default::default()
            };
            for tick in 0..checkpoint {
                super::step(&mut world, super::input(tick), &actions);
            }
            let mut restored = world.clone();
            let saved_bytes = serde_json::to_vec(&restored).unwrap();
            for tick in checkpoint..180 {
                super::step(&mut world, super::input(tick), &actions);
            }
            assert_eq!(
                serde_json::to_vec(&restored).unwrap(),
                saved_bytes,
                "snapshot aliases live state"
            );
            for tick in checkpoint..180 {
                super::step(&mut restored, super::input(tick), &actions);
            }
            assert_eq!(
                world, restored,
                "physics snapshot diverged at checkpoint {checkpoint}"
            );
            assert_eq!(world.hit_count, 1);
        }
    }
}
