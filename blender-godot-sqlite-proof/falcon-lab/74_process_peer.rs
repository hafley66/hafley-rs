//! One independently clocked GGRS peer. The existing GGRS UDP codec owns the wire format.
use crate::fixture::{
    self,
    sql_viewer::boundary::{Boundary, read_frame, contracts::PROTOCOL_VERSION},
};
use falcon_simulation::{World, fixture_input};
use ggrs::{
    Config, NonBlockingSocket, PlayerType, PredictRepeatLast, SessionBuilder, SessionState,
    UdpNonBlockingSocket,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Game;
impl Config for Game {
    type Input = u8;
    type InputPredictor = PredictRepeatLast;
    type State = World;
    type Address = SocketAddr;
}
// GGRS binds wildcard IPv4; reject incoming sources other than the loopback relay.
struct RelaySocket(UdpNonBlockingSocket, SocketAddr);
impl NonBlockingSocket<SocketAddr> for RelaySocket {
    fn send_to(&mut self, message: &ggrs::Message, address: &SocketAddr) {
        assert_eq!(address, &self.1);
        self.0.send_to(message, address);
    }
    fn receive_all_messages(&mut self) -> Vec<(SocketAddr, ggrs::Message)> {
        self.0
            .receive_all_messages()
            .into_iter()
            .filter(|(addr, _)| *addr == self.1)
            .collect()
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Frame {
    pub world: World,
    pub elapsed_us: u64,
    pub predicted: bool,
    pub input: u8,
    pub confirmed: i32,
    pub restored: Vec<i32>,
    pub advances: usize,
    pub generation: u64,
    pub rows: Vec<fixture::sql_viewer::boundary::Row>,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Recording {
    pub pid: u32,
    pub peer: usize,
    pub period_ms: u64,
    pub final_confirmed: i32,
    pub frames: Vec<Frame>,
    pub corrected: Vec<Option<World>>,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let i = args.iter().position(|v| v == "--process-peer").unwrap();
    let id: usize = args[i + 1].parse()?;
    assert!(id < 2);
    let port: u16 = args[i + 2].parse()?;
    let relay: SocketAddr = args[i + 3].parse()?;
    assert!(relay.ip().is_loopback());
    let actions = fixture::baseline::load()?;
    let baked = fixture::bake(&actions);
    let mut builder = SessionBuilder::<Game>::new()
        .with_num_players(2)?
        .with_max_prediction_window(32);
    for player in 0..2 {
        builder = builder.add_player(
            if player == id {
                PlayerType::Local
            } else {
                PlayerType::Remote(relay)
            },
            player,
        )?;
    }
    let mut session = builder.start_p2p_session(RelaySocket(
        UdpNonBlockingSocket::bind_to_port(port)?,
        relay,
    ))?;
    let watchdog = Instant::now();
    while session.current_state() != SessionState::Running {
        assert!(
            watchdog.elapsed() < Duration::from_secs(10),
            "handshake timeout"
        );
        session.poll_remote_clients();
        std::thread::sleep(Duration::from_millis(1));
    }
    std::fs::write(format!("ready-{id}"), std::process::id().to_string())?;
    while !std::path::Path::new("start-ms").exists() {
        assert!(watchdog.elapsed() < Duration::from_secs(12));
        session.poll_remote_clients();
        std::thread::sleep(Duration::from_millis(1));
    }
    let start_ms: u64 = std::fs::read_to_string("start-ms")?.parse()?;
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let start = Instant::now() + Duration::from_millis(start_ms.saturating_sub(now_ms));
    let period_ms = 40 + id as u64;
    let mut world = World {
        bag: Some(Default::default()),
        ..Default::default()
    };
    let mut boundary = Boundary::new()?;
    let mut frames = Vec::new();
    let mut corrected: Vec<Option<World>> = (0..200).map(|_| None).collect();
    let mut loads = 0;
    let live_path = std::env::var_os("FALCON_LIVE_DIR")
        .map(|dir| std::path::PathBuf::from(dir).join(format!("live-{id}.bin")));
    for tick in 0..200 {
        let deadline = start + Duration::from_millis(tick * period_ms);
        while Instant::now() < deadline {
            session.poll_remote_clients();
            std::thread::sleep(Duration::from_millis(1));
        }
        let _span = tracing::debug_span!(target: "falcon::network", "process_tick", peer=id, tick)
            .entered();
        session.add_local_input(
            id,
            if id == 0 {
                fixture_input(tick as i32)
            } else {
                0
            },
        )?;
        let requests = session.advance_frame()?;
        let mut display = fixture::handle(
            &mut world,
            requests,
            &actions,
            &mut loads,
            &baked,
            |state| {
                corrected[(state.frame - 1) as usize] = Some(state.clone());
            },
        );
        display.confirmed = session.confirmed_frame();
        display.stamp_presented();
        assert_eq!(world.frame, tick as i32 + 1);
        assert!(boundary.publish(&display.presented));
        let (generation, rows) = read_frame(&boundary.db, tick as i64)?;
        assert_eq!(rows, *display.presented.last().unwrap());
        if let Some(path) = &live_path {
            crate::live_rows::publish(
                path,
                &crate::live_rows::Latest {
                    version: PROTOCOL_VERSION,
                    pid: std::process::id(),
                    generation,
                    elapsed_us: start.elapsed().as_micros() as u64,
                    rows: rows.clone(),
                },
            )?;
            // Compact controller-only progress marker; it never gates simulation.
            std::fs::write(format!("progress-{id}"), tick.to_string())?;
        }
        frames.push(Frame {
            world: display.world,
            elapsed_us: start.elapsed().as_micros() as u64,
            predicted: display.predicted,
            input: display.applied,
            confirmed: session.confirmed_frame(),
            restored: display.restored,
            advances: display.advances,
            generation,
            rows,
        });
    }
    // Keep exchanging acknowledgements after both local timelines stop advancing.
    let drain = Instant::now();
    while drain.elapsed() < Duration::from_secs(2) {
        session.poll_remote_clients();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        session.confirmed_frame() >= 199,
        "last input was not confirmed"
    );
    let recording = Recording {
        pid: std::process::id(),
        peer: id,
        period_ms,
        final_confirmed: session.confirmed_frame(),
        frames,
        corrected,
    };
    std::fs::write(format!("peer-{id}.json"), serde_json::to_vec(&recording)?)?;
    Ok(())
}
