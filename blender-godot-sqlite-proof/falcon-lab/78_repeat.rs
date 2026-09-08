//! Two attacks in one continuous stationary-target world, through generated rows.
//! Uses existing Simulation/Parry hit detection and Capture; no new combat rules.
use crate::fixture::{self, baseline::{self, gpu, text}, sql_viewer};
use sql_viewer::boundary::{Boundary, Row, ROW_CAPACITY};
use sql_viewer::boundary::contracts::{FrameQuery, RowPublisher, FrameValues, RepeatProof, REPEAT_PERIOD, REPEAT_TICKS};
use falcon_simulation::{Simulation, World, fixture_input};

fn sequence() -> Result<(Vec<brawllib_rs::high_level_fighter::HighLevelSubaction>, Vec<World>, RepeatProof), Box<dyn std::error::Error>> {
    let actions = baseline::load()?;
    let mut sim = Simulation::new(fixture::bake(&actions).into(), false);
    let mut worlds = Vec::new();
    let mut checkpoint = None;
    // Capture before the second jump. Playback does not include the verification replay.
    let restore_tick = REPEAT_PERIOD as i32 + 60;
    for tick in 0..REPEAT_TICKS as i32 {
        if tick == restore_tick { checkpoint = Some(sim.save()); }
        worlds.push(sim.advance(fixture_input(tick % REPEAT_PERIOD as i32)).clone());
    }
    let hit_ticks: Vec<_> = worlds.iter().filter(|w| w.view.hit.is_some()).map(|w| i64::from(w.frame - 1)).collect();
    assert_eq!(hit_ticks, [91, 211]);
    assert_eq!((worlds.last().unwrap().hit_count, worlds.last().unwrap().damage), (2, 36.0));
    sim.load(checkpoint.as_ref().unwrap());
    for tick in restore_tick..REPEAT_TICKS as i32 {
        assert_eq!(sim.advance(fixture_input(tick % REPEAT_PERIOD as i32)), &worlds[tick as usize], "repeat replay tick {tick}");
    }
    let proof = RepeatProof { ticks: REPEAT_TICKS, hits: 2, damage: 36.0,
        replayed: REPEAT_TICKS - restore_tick as u32, hit_ticks };
    Ok((actions, worlds, proof))
}

#[tracing::instrument(target = "falcon::proof", level = "trace", skip_all, fields(record))]
pub fn run(record: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (actions, worlds, proof) = sequence()?;
    let mut boundary = Boundary::new()?;
    let mut output = vec![Row::new(0, 0, 0); ROW_CAPACITY];
    let mut capture = if record { Some(gpu::Capture::new("repeat.mp4")?) } else { None };
    for world in &worlds {
        let tick = world.frame - 1;
        let input = fixture_input(tick % REPEAT_PERIOD as i32);
        let rows = sql_viewer::encode(world, &actions, false, input);
        let id = RowPublisher::publish(&mut boundary, &rows).unwrap();
        let read = FrameQuery::read_frame(&mut boundary, i64::from(tick), &mut output).unwrap();
        let observed = &output[..read.rows_written as usize];
        assert_eq!(read.id, id);
        assert_eq!(observed, rows);
        let meta = FrameValues::from_row(&observed[0]).unwrap();
        assert_eq!((meta.hits, meta.damage), (world.hit_count as f64, world.damage as f64));
        if let Some(capture) = &mut capture {
            let mut vertices = sql_viewer::draw(observed);
            let labels = [
                "TWO KNEES / ONE CONTINUOUS RUST WORLD".to_string(),
                format!("TICK {tick:03} / {} POSE {:02} / INPUT {input}", ["IDLE", "JUMP", "FAIR"][meta.action as usize], meta.pose as usize + 1),
                format!("HITS {:.0} / DAMAGE {:.0} / CONTACT {}", meta.hits, meta.damage, meta.contact != 0.0),
                format!("TSP ROWS -> SQLITE -> WGPU / GEN {} / EXACT", id.generation),
                "STATIONARY TARGET / SCRIPTED TRAVEL / NO LAUNCH".to_string(),
                format!("SNAPSHOT 180 -> 299 REPLAY: {} STATES EXACT", proof.replayed),
            ];
            for (i, label) in labels.iter().enumerate() {
                text(&mut vertices, label, 24.0, 18.0 + i as f32 * 27.0, 1.7, [0.85, 0.91, 0.98, 1.0]);
            }
            text(&mut vertices, "0.5X PLAYBACK + EVENT HOLDS / SAME DAMAGE COUNTER", 24.0, 518.0, 1.25, [0.25, 0.85, 0.9, 1.0]);
            let hold = if [60, 78, 91, 180, 198, 211, 299].contains(&tick) { 45 } else { 2 };
            for _ in 0..hold {
                capture.frame_regions(&vertices, [[40, 400, 210, 430], [328, 396, 220, 312]])?;
            }
        }
    }
    if let Some(capture) = capture { capture.finish()?; }
    std::fs::write("repeat-proof.json", serde_json::to_vec_pretty(&proof)?)?;
    println!("REPEAT_OK ticks={} hits={} damage={} replayed={} sql_rows=exact", proof.ticks, proof.hits, proof.damage, proof.replayed);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn second_attack_preserves_damage_and_replays_exactly() {
        let (_, worlds, proof) = super::sequence().unwrap();
        assert_eq!(worlds[210].damage, 18.0);
        assert_eq!(worlds[211].damage, 36.0);
        assert_eq!(proof.replayed, 120);
    }
}
