//! Lab scheduling harness. std threads/Instant pace execution; rusqlite cursors
//! and the existing core_labs FrameRing supply reader leases and recycling.
use smash::fighters::falcon;
use crate::fixture::{
    self, Runtime,
    sql_viewer::boundary::{self, Boundary, Ring, Row},
};
use serde::Deserialize;
#[cfg(feature = "gdext")]
use std::time::{Duration, Instant};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

type Error = Box<dyn std::error::Error>;
pub use boundary::contracts::ScheduledStatus as Status;
#[derive(Default)]
pub struct Shared {
    pub ring: Option<Ring>,
    pub current: Option<Status>,
    pub published: Option<Status>,
}
pub type State = Arc<Mutex<Shared>>;

#[derive(Deserialize)]
struct Recorded {
    world: falcon::World,
}

/// Clock is called before each fixed simulation step. Neither publication nor
/// renderer consumption determines whether that step executes.
pub fn run(
    stalled: bool,
    shared: &State,
    mut clock: impl FnMut(usize),
) -> Result<Vec<Status>, Error> {
    let actions = fixture::baseline::load()?;
    let golden: Vec<[Recorded; 2]> =
        serde_json::from_slice(include_bytes!("17_launch_trace.json"))?;
    let mut runtime = Runtime::new(&actions, true, true)?;
    let mut boundary = Boundary::new()?;
    shared.lock().unwrap().ring = Some(boundary.ring.clone());
    let readers = [boundary.reader()?, boundary.reader()?, boundary.reader()?];
    let mut a = readers[0].prepare("SELECT * FROM presentation WHERE tick=90")?;
    let mut b = readers[1].prepare("SELECT * FROM presentation WHERE tick=91")?;
    let mut c = readers[2].prepare("SELECT * FROM presentation WHERE tick=92")?;
    let mut statements = [Some(&mut a), Some(&mut b), Some(&mut c)];
    let mut pins: [Option<rusqlite::Rows<'_>>; 3] = [None, None, None];
    let mut held_expected = Vec::new();
    let mut held_rows = Vec::new();
    let mut held_generation = None;
    let mut pending: VecDeque<Vec<Row>> = VecDeque::new();
    let mut audit = Vec::new();
    let mut skipped = 0;
    let mut published_tick = -1;
    for (tick, expected) in golden.iter().enumerate() {
        clock(tick);
        let pair = runtime.advance(falcon::fixture_input(tick as i32))?;
        assert_eq!(runtime.tick(), tick + 1);
        for (actual, expected) in pair.iter().zip(expected) {
            assert_eq!(
                actual.world, expected.world,
                "authoritative state at tick {tick}"
            );
        }
        let display = &pair[1];
        let first = display.presented.first().unwrap()[0].tick;
        pending.retain(|frame| frame[0].tick < first);
        pending.extend(display.presented.iter().cloned());
        while pending.len() > boundary::WINDOW as usize {
            pending.pop_front();
        }
        if stalled && tick == 105 {
            pins[0].take();
            pins[2].take();
        }
        if stalled && tick == 110 {
            let mut cursor = pins[1].take().unwrap();
            while let Some(row) = cursor.next()? {
                let (generation, row) = boundary::read_row(row)?;
                assert_eq!(Some(generation), held_generation);
                held_rows.push(row);
            }
            assert_eq!(held_rows, held_expected);
            held_generation = None;
        }
        // This short lock makes published metadata and its SQL generation one
        // observation. No GPU work or wait occurs while holding it.
        let mut state = shared.lock().unwrap();
        let published = boundary.publish(pending.make_contiguous());
        if published {
            published_tick = tick as i64;
        } else {
            skipped += 1;
        }
        assert_eq!(published, !stalled || !(93..105).contains(&tick));
        if stalled {
            let statement = if (90..=92).contains(&tick) {
                statements[tick - 90].take()
            } else {
                None
            };
            if let Some(statement) = statement {
                let mut cursor = statement.query([])?;
                let (generation, row) = boundary::read_row(cursor.next()?.unwrap())?;
                if tick == 91 {
                    assert_eq!(row.values[5], 0.0);
                    held_generation = Some(generation);
                    held_expected = display.presented.last().unwrap().clone();
                    held_rows.push(row);
                }
                pins[tick - 90] = Some(cursor);
            }
        }
        let fresh = if (91..123).contains(&published_tick) {
            Some(boundary::read_frame(&boundary.db, 91)?.1[0].values[5])
        } else {
            None
        };
        if published {
            let (generation, rows) = boundary::read_frame(&boundary.db, tick as i64)?;
            assert_eq!(rows, *display.presented.last().unwrap());
            assert_eq!(generation, boundary.generation);
            let mut statement = boundary.db.prepare("SELECT * FROM presentation")?;
            let stored = statement
                .query_map([], boundary::read_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let expected: Vec<_> = pending
                .iter()
                .flatten()
                .map(|row| (generation, *row))
                .collect();
            assert_eq!(
                stored, expected,
                "complete corrected SQL window at tick {tick}"
            );
            if tick == 105 {
                assert_eq!(fresh, Some(18.0));
            }
        }
        let status = Status {
            simulation_tick: tick as i64,
            published_tick,
            generation: boundary.generation,
            skipped_publications: skipped,
            published,
            advances: u32::try_from(display.advances)?,
            restored: display.restored.clone(),
            held_generation,
            held_damage: held_generation.map(|_| held_rows[0].values[5]),
            fresh_tick91_damage: fresh,
            rows: u32::try_from(boundary.ring.read().unwrap().current().rows.len())?,
            window_frames: u32::try_from(pending.len())?,
        };
        if published {
            state.published = Some(status.clone());
        }
        state.current = Some(status.clone());
        audit.push(status);
    }
    assert_eq!(skipped, if stalled { 12 } else { 0 });
    assert_eq!(audit[97].restored, [78]);
    assert_eq!(audit[97].advances, 20);
    assert_eq!(boundary.layout, boundary.ring.read().unwrap().slot_layout());
    Ok(audit)
}

#[cfg(feature = "gdext")]
pub fn spawn(shared: State, faults: bool) -> std::thread::JoinHandle<Result<Vec<Status>, String>> {
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let parent = tracing::Span::current();
    std::thread::spawn(move || {
        let _dispatch = tracing::dispatcher::set_default(&dispatch);
        let worker =
            tracing::info_span!(target: "falcon::worker", parent: &parent, "worker", faults);
        // Lifecycle span includes pacing waits. Per-operation spans measure work.
        let _worker = worker.enter();
        let mut start = None;
        run(!faults, &shared, |tick| {
            let start = *start.get_or_insert_with(Instant::now);
            // 1/60 simulation steps at 25 steps/second for legible capture.
            let due = start + Duration::from_millis(tick as u64 * 40);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            tracing::debug!(target: "falcon::worker", parent: &worker, tick, elapsed_us = start.elapsed().as_micros() as u64, "tick_due");
            if faults {
                println!("WORKER_TICK {tick} {}", start.elapsed().as_micros());
            }
        })
        .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_schedules_preserve_states_and_recover_latest_generation() {
        let baseline = run(false, &State::default(), |_| {}).unwrap();
        let shared = State::default();
        let stalled = run(true, &shared, |tick| {
            // A consumer can stop observing entirely; the next step still runs.
            if tick > 0 && !(92..106).contains(&tick) {
                let state = shared.lock().unwrap();
                let current = state.published.as_ref().unwrap();
                let reader = boundary::reader_for(state.ring.as_ref().unwrap()).unwrap();
                let (generation, rows) =
                    boundary::read_frame(&reader, current.published_tick).unwrap();
                assert_eq!(generation, current.generation);
                assert_eq!(rows[0].tick, current.published_tick);
            }
        })
        .unwrap();
        assert_eq!(baseline.len(), 180);
        assert_eq!(stalled.len(), 180);
        assert_eq!(
            (stalled[104].simulation_tick, stalled[104].published_tick),
            (104, 92)
        );
        assert_eq!(
            (
                stalled[105].published_tick,
                stalled[105].skipped_publications
            ),
            (105, 12)
        );
        assert_eq!(
            (stalled[105].held_damage, stalled[105].fresh_tick91_damage),
            (Some(0.0), Some(18.0))
        );
        assert_eq!(stalled[110].held_generation, None);
        for (a, b) in baseline.iter().zip(&stalled) {
            assert_eq!((&a.restored, a.advances), (&b.restored, b.advances));
        }
    }
}
