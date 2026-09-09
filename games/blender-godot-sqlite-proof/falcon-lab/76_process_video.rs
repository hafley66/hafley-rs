//! Validate recorded process state, then render its existing SQL wire geometry.
use smash::fighters::falcon;
use crate::{
    fixture::{
        baseline::{gpu, text},
        sql_viewer,
    },
    process_peer::Recording,
};
use serde::Deserialize;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let peers: [Recording; 2] = [
        serde_json::from_slice(&std::fs::read("peer-0.json")?)?,
        serde_json::from_slice(&std::fs::read("peer-1.json")?)?,
    ];
    #[derive(Deserialize)]
    struct Golden {
        world: falcon::World,
    }
    let golden: Vec<[Golden; 2]> = serde_json::from_slice(include_bytes!("17_launch_trace.json"))?;
    assert_ne!(peers[0].pid, peers[1].pid);
    let mut restores = Vec::new();
    let mut differences = Vec::new();
    for (id, peer) in peers.iter().enumerate() {
        assert_eq!(peer.peer, id);
        assert_eq!(peer.frames.len(), 200);
        assert_eq!(peer.corrected.len(), 200);
        assert!(peer.final_confirmed >= 199);
        for (tick, expected) in golden.iter().enumerate() {
            assert_eq!(
                peer.corrected[tick].as_ref().unwrap(),
                &expected[0].world,
                "peer {id} corrected tick {tick}"
            );
        }
        for (tick, frame) in peer.frames.iter().enumerate() {
            assert_eq!(frame.world.frame, tick as i32 + 1);
            assert_eq!(frame.generation, tick as u64 + 1);
            assert_eq!(frame.rows[0].values[18], frame.confirmed as f64);
            if frame.confirmed >= tick as i32 {
                assert_eq!(&frame.world, peer.corrected[tick].as_ref().unwrap());
            }
            if !frame.restored.is_empty() {
                restores.push(serde_json::json!({"peer":id,"tick":tick,"restore":frame.restored,"advances":frame.advances}));
            }
        }
    }
    for tick in 0..200 {
        assert_eq!(peers[0].corrected[tick], peers[1].corrected[tick]);
        if peers[0].frames[tick].world != peers[1].frames[tick].world {
            differences.push(tick);
        }
    }
    assert!(!restores.is_empty());
    assert!(!differences.is_empty());
    assert!(differences.iter().any(|tick| (91..110).contains(tick)));
    assert_eq!(peers[0].frames[199].world, peers[1].frames[199].world);
    assert_eq!(peers[1].frames[199].world.damage, 18.0);
    assert!(peers[1].frames[199].elapsed_us > peers[0].frames[199].elapsed_us + 100_000);
    std::fs::write(
        "verification.json",
        serde_json::to_vec_pretty(&serde_json::json!({
            "pids":[peers[0].pid,peers[1].pid], "corrected_worlds_match_golden":360,
            "corrected_peer_pairs_equal":200,"speculative_different_ticks":differences,
            "restores":restores,"final_damage":18,"final_confirmed":[peers[0].final_confirmed,peers[1].final_confirmed]
        }))?,
    )?;
    if std::env::args().any(|a| a == "--verify-only") {
        return Ok(());
    }
    let white = [0.85, 0.91, 0.98, 1.0];
    let cyan = [0.25, 0.85, 0.9, 1.0];
    let orange = [1.0, 0.42, 0.18, 1.0];
    let mut capture = gpu::Capture::new("processes.mp4")?;
    for tick in 0..200 {
        let mut vertices = Vec::new();
        for (id, peer) in peers.iter().enumerate() {
            let f = &peer.frames[tick];
            let mut mesh = sql_viewer::draw(&f.rows);
            for v in &mut mesh {
                let x = (v[0] + 1.0) * 480.0;
                let y = (1.0 - v[1]) * 270.0;
                v[0] = (x * 0.48 + id as f32 * 480.0) / 480.0 - 1.0;
                v[1] = 1.0 - (y * 0.8 + 25.0) / 270.0;
            }
            vertices.extend(mesh);
            let x = 14.0 + id as f32 * 480.0;
            let bag = f.world.bag.as_ref().unwrap();
            for (line, label) in [
                format!(
                    "PEER {} / PID {} / {}MS CLOCK",
                    id, peer.pid, peer.period_ms
                ),
                format!("TICK {tick:03} / CONFIRMED {}", f.confirmed),
                format!(
                    "{} POSE {} / INPUT {} {}",
                    ["IDLE", "JUMP", "FAIR"][f.world.view.action],
                    f.world.view.frame + 1,
                    f.input,
                    if f.predicted { "PREDICTED" } else { "KNOWN" }
                ),
                format!(
                    "BAG {} / DAMAGE {:.0} / STUN {}",
                    bag.phase.label(),
                    f.world.damage,
                    bag.stun
                ),
                format!(
                    "RESTORE {:?} / REPLAY {} / STEP 1",
                    f.restored,
                    f.advances - 1
                ),
                format!(
                    "WALL {:.3}S / SQL GEN {}",
                    f.elapsed_us as f64 / 1e6,
                    f.generation
                ),
            ]
            .iter()
            .enumerate()
            {
                text(
                    &mut vertices,
                    label,
                    x,
                    16.0 + line as f32 * 24.0,
                    1.25,
                    if f.restored.is_empty() { white } else { orange },
                );
            }
        }
        let equal = peers[0].frames[tick].world == peers[1].frames[tick].world;
        text(
            &mut vertices,
            if equal {
                "SAME-TICK SPECULATIVE WORLDS MATCH"
            } else {
                "SAME-TICK SPECULATIVE WORLDS DIFFER"
            },
            20.0,
            405.0,
            1.8,
            if equal { cyan } else { orange },
        );
        text(
            &mut vertices,
            "UDP RELAY: 20/30/40MS DELAY / DROP EVERY 17TH PACKET",
            20.0,
            438.0,
            1.5,
            white,
        );
        text(
            &mut vertices,
            "A->B HOLD: WALL 3.050..4.070S / SEPARATE PROCESS TIMERS",
            20.0,
            467.0,
            1.4,
            cyan,
        );
        text(
            &mut vertices,
            "POST-RUN: CONFIRMED 199/199 / 200 PAIRS MATCH / GOLDEN 360 PASS",
            20.0,
            493.0,
            1.3,
            white,
        );
        text(
            &mut vertices,
            "RECORDED SQL STATES / TICK-ALIGNED 0.5X + EVENT HOLDS / DT 1/60",
            20.0,
            520.0,
            1.1,
            white,
        );
        let event = peers.iter().any(|p| !p.frames[tick].restored.is_empty())
            || [60, 78, 91, 199].contains(&tick);
        for _ in 0..if event { 45 } else { 2 } {
            capture.frame_regions(&vertices, [[0, 960, 170, 395], [0, 960, 170, 395]])?;
        }
    }
    capture.finish()
}
