#[path = "1b_boundary.rs"]
pub mod boundary;
#[path = "27_geometry.rs"]
pub mod geometry;
use super::{CYAN, Display, Error, ORANGE, WHITE, World, baseline};
use baseline::{gpu, project, text};
use boundary::{Boundary, Row};
use brawllib_rs::high_level_fighter::{CollisionBoxValues, HighLevelSubaction};

pub(super) fn encode(
    world: &World,
    actions: &[HighLevelSubaction],
    predicted: bool,
    applied: u8,
) -> Vec<Row> {
    let s = &world.view;
    let source = &actions[s.action].frames[s.frame];
    let tick = i64::from(world.frame - 1);
    let mut meta = Row::new(tick, 0, 0);
    meta.values[..15].copy_from_slice(&[
        s.action as f64,
        s.frame as f64,
        s.root[0] as f64,
        s.root[1] as f64,
        s.root[2] as f64,
        world.damage as f64,
        world.hit_count as f64,
        world.last_hit.unwrap_or(-1) as f64,
        f64::from(s.contact),
        f64::from(predicted),
        f64::from(applied),
        source.x_pos as f64,
        source.y_pos as f64,
        0.0,
        0.0,
    ]);
    let mut rows = vec![meta];
    let mut target = Row::new(tick, 1, 0);
    if let Some(b) = &world.bag {
        for i in 0..3 {
            target.values[i] = b.position[i] as f64;
            target.values[i + 3] = b.velocity[i] as f64;
        }
        target.values[6] = b.stun as f64;
        target.values[7] = match b.phase {
            super::sandbag::Phase::Hovering => 0.0,
            super::sandbag::Phase::Hit => 1.0,
            super::sandbag::Phase::Hitstun => 2.0,
            super::sandbag::Phase::Falling => 3.0,
            super::sandbag::Phase::Landed => 4.0,
        };
        target.values[8] = f64::from(b.grounded);
    } else {
        target.values[1] = 24.0;
        target.values[2] = 28.0;
    }
    rows.push(target);
    for hurt in &source.hurt_boxes {
        let mut r = Row::new(tick, 2, hurt.hurt_box.bone_index as i64);
        let matrix: [[f32; 4]; 4] = hurt.bone_matrix.into();
        for (out, value) in r.values.iter_mut().zip(matrix.iter().flatten()) {
            *out = *value as f64;
        }
        let a = hurt.hurt_box.offset;
        let b = hurt.hurt_box.stretch;
        r.values[16..].copy_from_slice(&[
            a.x as f64,
            a.y as f64,
            a.z as f64,
            b.x as f64,
            b.y as f64,
            b.z as f64,
            hurt.hurt_box.radius as f64,
            f64::from(hurt.hurt_box.enabled),
        ]);
        rows.push(r);
    }
    for hit in &source.hit_boxes {
        let CollisionBoxValues::Hit(values) = &hit.next_values else {
            continue;
        };
        let mut r = Row::new(tick, 3, hit.hitbox_id as i64);
        r.values[..6].copy_from_slice(&[
            hit.next_pos.x as f64,
            hit.next_pos.y as f64,
            hit.next_pos.z as f64,
            hit.next_size as f64,
            values.damage as f64,
            f64::from(values.enabled),
        ]);
        rows.push(r);
    }
    assert!(rows.iter().all(|r| r.values.iter().all(|v| v.is_finite())));
    rows
}

fn draw(rows: &[Row]) -> Vec<gpu::Vertex> {
    let meta = &rows.iter().find(|r| r.kind == 0).unwrap().values;
    let target = &rows.iter().find(|r| r.kind == 1).unwrap().values;
    let mut out = Vec::new();
    for line in geometry::wire(rows) {
        gpu::line(
            &mut out,
            project(line.a.into()),
            project(line.b.into()),
            line.width,
            line.color,
        );
    }
    for v in &mut out {
        let x = (v[0] + 1.0) * 480.0;
        let y = (1.0 - v[1]) * 270.0;
        v[0] = (180.0 + (x - 350.0) * 0.65) / 480.0 - 1.0;
        v[1] = 1.0 - (422.0 + (y - 444.0) * 0.65) / 270.0;
    }
    text(
        &mut out,
        &format!("BAG {:.0}%", meta[5]),
        180.0 + target[2] as f32 * 6.5 - 25.0,
        422.0 - target[1] as f32 * 6.5 - 52.0,
        1.3,
        CYAN,
    );
    out
}

pub(super) fn execute(trace: &[[Display; 2]], record: bool) -> Result<(), Error> {
    execute_with(trace, record, |_, _| Ok(()))
}

pub(super) fn execute_with(
    trace: &[[Display; 2]],
    record: bool,
    mut consume: impl FnMut(&[Row], &serde_json::Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut b = Boundary::new()?;
    let reader = b.reader()?;
    let mut statement = reader.prepare("SELECT * FROM presentation WHERE tick=91")?;
    let mut statement_once = Some(&mut statement);
    let mut held = None;
    let mut pinned_rows = Vec::new();
    let mut held_generation = 0;
    let mut held_expected = Vec::new();
    let mut held_pointer = 0;
    let mut released_slot_reused = false;
    let mut report = Vec::new();
    let mut max_rows = 0;
    let mut capture = if record {
        Some(gpu::Capture::new("23_sql_boundary.mp4")?)
    } else {
        None
    };
    for (tick, pair) in trace.iter().enumerate() {
        let d = &pair[1];
        assert!(b.publish(&d.presented));
        let current_pointer = b.ring.read().unwrap().current().rows.as_ptr() as usize;
        if tick > 101 && current_pointer == held_pointer {
            released_slot_reused = true;
        }
        if (92..=101).contains(&tick) {
            assert_ne!(current_pointer, held_pointer);
        }
        if tick == 91 {
            let mut cursor = statement_once.take().unwrap().query([])?;
            let (generation, row) = boundary::read_row(cursor.next()?.unwrap())?;
            held_generation = generation;
            pinned_rows.push(row);
            held = Some(cursor);
            held_pointer = current_pointer;
            held_expected = d.presented.last().unwrap().clone();
            assert_eq!(row.values[5], 0.0);
        }
        let mut released = false;
        if tick == 101 {
            let mut cursor = held.take().unwrap();
            while let Some(row) = cursor.next()? {
                let (generation, r) = boundary::read_row(row)?;
                assert_eq!(generation, held_generation);
                pinned_rows.push(r);
            }
            assert_eq!(pinned_rows, held_expected);
            released = true;
        }
        let (generation, rows) = boundary::read_frame(&b.db, tick as i64)?;
        assert_eq!(generation, b.generation);
        assert_eq!(
            &rows,
            d.presented.last().unwrap(),
            "SQL roundtrip changed the presentation"
        );
        let count: i64 =
            b.db.query_row("SELECT count(*) FROM presentation", [], |r| r.get(0))?;
        max_rows = max_rows.max(count);
        assert!(count <= boundary::ROW_CAPACITY as i64);
        let frames: i64 =
            b.db.query_row("SELECT count(DISTINCT tick) FROM presentation", [], |r| {
                r.get(0)
            })?;
        assert!(frames <= boundary::WINDOW);
        let corrected = if (91..123).contains(&tick) {
            Some(
                b.db.query_row("SELECT damage FROM frame_state WHERE tick=91", [], |r| {
                    r.get::<_, f64>(0)
                })?,
            )
        } else {
            None
        };
        if (97..123).contains(&tick) {
            assert_eq!(corrected, Some(18.0));
        }
        if tick == 97 {
            for (historical, historical_pair) in trace.iter().enumerate().take(98).skip(78) {
                let (_, actual) = boundary::read_frame(&b.db, historical as i64)?;
                let expected = historical_pair[0].presented.last().unwrap();
                assert_eq!(actual.len(), expected.len());
                for (a, e) in actual.iter().zip(expected) {
                    assert_eq!((a.tick, a.kind, a.entity), (e.tick, e.kind, e.entity));
                    if a.kind == 0 {
                        assert_eq!(a.values[..9], e.values[..9]);
                        assert_eq!(a.values[11..15], e.values[11..15]);
                    } else {
                        assert_eq!(a, e);
                    }
                }
            }
        }
        let meta = &rows[0].values;
        let target = &rows[1].values;
        if let Some(capture) = &mut capture {
            let mut vertices = draw(&rows);
            text(
                &mut vertices,
                "FALCON -> RECYCLED SQLITE -> WGPU",
                24.0,
                16.0,
                2.3,
                WHITE,
            );
            text(
                &mut vertices,
                &format!(
                    "SIM {:03} / PUBLISHED {:03} / RENDERER {:03}",
                    tick, b.generation, generation
                ),
                24.0,
                49.0,
                1.7,
                CYAN,
            );
            text(
                &mut vertices,
                &format!(
                    "{} POSE {:02} / INPUT {}",
                    ["IDLE", "JUMP", "FAIR"][meta[0] as usize],
                    meta[1] as usize + 1,
                    if meta[9] != 0.0 {
                        "PREDICTED"
                    } else {
                        "CONFIRMED"
                    }
                ),
                24.0,
                76.0,
                1.7,
                WHITE,
            );
            text(
                &mut vertices,
                &format!(
                    "BAG {} / {:.0}% / STUN {:.0}",
                    ["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][target[7] as usize],
                    meta[5],
                    target[6]
                ),
                24.0,
                103.0,
                1.7,
                ORANGE,
            );
            text(
                &mut vertices,
                &format!("SQL WINDOW {frames}/32 / ROWS {count}/1024 / SLOTS 3"),
                24.0,
                130.0,
                1.5,
                CYAN,
            );
            text(
                &mut vertices,
                "BONES + CAPSULES + TARGET READ THROUGH SQL",
                24.0,
                157.0,
                1.3,
                WHITE,
            );
            let status = if meta[15] >= 0.0 {
                format!("RESTORE {:.0} / ATOMIC CORRECTION: 20 FRAMES", meta[15])
            } else {
                "PUBLISH COMPLETE WINDOW / SLOT POINTERS STABLE".into()
            };
            text(&mut vertices, &status, 24.0, 438.0, 1.4, WHITE);
            let reader_label = if held.is_some() {
                format!("HELD SQL CURSOR: GEN {held_generation} / TICK 91 / DAMAGE 0")
            } else if released {
                "HELD CURSOR RELEASED / SLOT REUSABLE".into()
            } else {
                "HELD SQL CURSOR: NONE".into()
            };
            text(&mut vertices, &reader_label, 24.0, 464.0, 1.5, ORANGE);
            text(
                &mut vertices,
                &corrected.map_or("LATEST GENERATION / BOUNDED STORAGE".into(), |v| {
                    format!("FRESH SQL QUERY: TICK 91 DAMAGE {v:.0}")
                }),
                24.0,
                491.0,
                1.5,
                CYAN,
            );
            text(
                &mut vertices,
                "0.5X + HOLDS / SCRIPTED TRAVEL / PM + MELEE KB + RAPIER",
                24.0,
                518.0,
                1.1,
                WHITE,
            );
            let x = 180.0 + target[2] as f32 * 6.5;
            let y = 422.0 - target[1] as f32 * 6.5;
            let repeats = if [60, 78, 91, 97, 101, 117, 126, 179].contains(&tick) {
                60
            } else {
                2
            };
            for _ in 0..repeats {
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
            }
        }
        report.push(serde_json::json!({"simulation_tick":tick,"published_generation":b.generation,"renderer_generation":generation,"rows":count,"window_frames":frames,"held_generation":if held.is_some(){Some(held_generation)}else{None},"held_damage":if held.is_some(){Some(0)}else{None},"fresh_tick91_damage":corrected,"restored":d.restored}));
        consume(&rows, report.last().unwrap())?;
    }
    assert_eq!(b.layout, b.ring.read().unwrap().slot_layout());
    assert!(
        released_slot_reused,
        "released SQL cursor slot was never recycled"
    );
    assert_eq!(
        b.db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='index'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    assert!(b.db.execute("DELETE FROM presentation", []).is_err());
    std::fs::write(
        "24_sql_boundary_trace.json",
        serde_json::to_vec_pretty(&report)?,
    )?;
    if let Some(c) = capture {
        c.finish()?;
    }
    eprintln!(
        "SQL_BOUNDARY_OK generations={} max_rows={max_rows} slots=3 capacity=1024 held_gen=92 old_damage=0 corrected_damage=18",
        b.generation
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn replay_publication_and_held_sql_reader() {
        let actions = super::baseline::load().unwrap();
        let trace = super::super::run(&actions, true, true).unwrap();
        super::execute(&trace, false).unwrap();
    }
}
