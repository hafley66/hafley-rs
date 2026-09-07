// Reuse the preceding fixture's decoder and wireframe presentation unchanged.
#[allow(dead_code)]
#[path = "2_main.rs"]
mod baseline;
use baseline::{Tick, gpu, text};
use brawllib_rs::high_level_fighter::{CollisionBoxValues, HighLevelSubaction};
use ggrs::{
    Config, GgrsRequest, InputStatus, Message, NonBlockingSocket, PlayerType, PredictRepeatLast,
    SessionBuilder, SessionState,
};
use parry3d::{
    math::{Pose, Vec3},
    query,
    shape::{Ball, Cuboid},
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

type Error = Box<dyn std::error::Error>;
const WHITE: [f32; 4] = [0.85, 0.91, 0.98, 1.0];
const CYAN: [f32; 4] = [0.25, 0.85, 0.9, 1.0];
const ORANGE: [f32; 4] = [1.0, 0.42, 0.18, 1.0];
const JUMP: u8 = 1;
const ATTACK: u8 = 2;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct World {
    frame: i32,
    action: usize,
    animation: usize,
    jump_at: Option<i32>,
    previous_input: u8,
    attack_hit: bool,
    damage: f32,
    hit_count: usize,
    last_hit: Option<i32>,
    view: Tick,
}

fn input(tick: i32) -> u8 {
    match tick {
        60 => JUMP,
        78 => ATTACK,
        _ => 0,
    }
}

fn step(world: &mut World, bits: u8, actions: &[HighLevelSubaction]) {
    let pressed = bits & !world.previous_input;
    if pressed & JUMP != 0 && world.view.root[1] == 0.0 {
        world.jump_at = Some(world.frame);
        world.action = 1;
        world.animation = 0;
    }
    if pressed & ATTACK != 0 && world.jump_at.is_some() && world.view.root[1] > 0.0 {
        world.action = 2;
        world.animation = 0;
        world.attack_hit = false;
    }
    let air = world.jump_at.map_or(0.0, |t| (world.frame - t) as f32);
    let root = [
        0.0,
        (0.95 * air - 0.018 * air * air).max(0.0),
        -12.0 + (0.9 * air).min(32.0),
    ];
    if world.action == 1 && air > 0.0 && root[1] == 0.0 {
        world.action = 0;
        world.animation = 0;
    }
    let frame = world.animation.min(actions[world.action].frames.len() - 1);
    let source = &actions[world.action].frames[frame];
    let mut view = Tick {
        action: world.action,
        frame,
        root,
        damage: world.damage,
        contact: false,
        hit: None,
    };
    for hb in &source.hit_boxes {
        let CollisionBoxValues::Hit(values) = &hb.next_values else {
            continue;
        };
        if !values.enabled || !values.aerial {
            continue;
        }
        let p = hb.next_pos;
        let overlap = query::intersection_test(
            &Pose::translation(
                p.x,
                p.y + root[1] + source.y_pos,
                p.z + root[2] + source.x_pos,
            ),
            &Ball::new(hb.next_size),
            &Pose::translation(0.0, 24.0, 28.0),
            &Cuboid::new(Vec3::new(3.0, 6.0, 4.0)),
        )
        .unwrap();
        view.contact |= overlap;
        if overlap && !world.attack_hit {
            world.damage += values.damage;
            world.attack_hit = true;
            world.hit_count += 1;
            world.last_hit = Some(world.frame);
            view.hit = Some((hb.hitbox_id, values.damage));
        }
    }
    view.damage = world.damage;
    world.view = view;
    world.previous_input = bits;
    world.frame += 1;
    world.animation += 1;
    if world.action == 2 && world.animation == actions[2].frames.len() {
        world.action = 0;
        world.animation = 0;
    }
    if world.action == 0 {
        world.animation %= actions[0].frames.len();
    }
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
struct Display {
    world: World,
    applied: u8,
    predicted: bool,
    confirmed: i32,
    restored: Vec<i32>,
    advances: usize,
    total_loads: usize,
}
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
) -> Display {
    let mut restored = Vec::new();
    let mut advances = 0;
    let mut predicted = false;
    let mut applied = 0;
    for request in requests {
        match request {
            GgrsRequest::SaveGameState { cell, frame } => {
                assert_eq!(frame, world.frame);
                cell.save(frame, Some(world.clone()), Some(checksum(world)));
            }
            GgrsRequest::LoadGameState { cell, frame } => {
                *world = cell.load().unwrap();
                assert_eq!(world.frame, frame);
                restored.push(frame);
                *loads += 1;
            }
            GgrsRequest::AdvanceFrame { inputs } => {
                applied = inputs[0].0;
                predicted = inputs[0].1 == InputStatus::Predicted;
                step(world, applied, actions);
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
        advances,
        total_loads: *loads,
    }
}

fn run(actions: &[HighLevelSubaction], held: bool) -> Result<Vec<[Display; 2]>, Error> {
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
    let mut loads = [0, 0];
    let mut trace = Vec::new();
    for tick in 0..180 {
        bus.lock().unwrap().tick = tick;
        for peer in &mut peers {
            peer.poll_remote_clients();
        }
        let mut displays = Vec::new();
        for id in 0..2 {
            assert_eq!(peers[id].current_frame(), tick as i32);
            peers[id].add_local_input(id, if id == 0 { input(tick as i32) } else { 0 })?;
            let requests = peers[id].advance_frame()?;
            let mut display = handle(&mut worlds[id], requests, actions, &mut loads[id]);
            display.confirmed = peers[id].confirmed_frame();
            displays.push(display);
        }
        trace.push(displays.try_into().unwrap());
    }
    Ok(trace)
}

fn verify(actions: &[HighLevelSubaction], trace: &[[Display; 2]]) -> Result<(), Error> {
    let mut reference = World::default();
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
    let clean = run(actions, false)?;
    assert!(clean.iter().all(|pair| pair[0].world == pair[1].world));
    let repeated = run(actions, true)?;
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
    Ok(())
}

fn render(actions: &[HighLevelSubaction], trace: &[[Display; 2]], id: usize) -> Result<(), Error> {
    let mut capture = gpu::Capture::new(&format!("10_peer{id}.mp4"))?;
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
        );
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
            "0.5X PLAYBACK / EVENT HOLDS / SCRIPTED TRAVEL",
            24.0,
            518.0,
            1.1,
            WHITE,
        );
        // Presentation-only holds preserve actual tick state, making events readable.
        let repeats = if [60, 78, 91, 97, 179].contains(&tick) {
            60
        } else {
            2
        };
        for _ in 0..repeats {
            capture.frame(&vertices)?;
        }
    }
    capture.finish()
}

fn main() -> Result<(), Error> {
    let actions = baseline::load()?;
    let trace = run(&actions, true)?;
    verify(&actions, &trace)?;
    std::fs::write("11_rollback_trace.json", serde_json::to_vec_pretty(&trace)?)?;
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
        let trace = super::run(&actions, true).unwrap();
        super::verify(&actions, &trace).unwrap();
    }
}
