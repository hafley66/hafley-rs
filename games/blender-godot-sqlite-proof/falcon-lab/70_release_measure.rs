//! Headless wall-clock measurements. No subscriber is installed in this CLI mode.
use crate::{Runtime, fixture};
use serde::{Deserialize, Serialize};
use std::{hint::black_box, time::Instant};

#[derive(Deserialize)]
struct Recorded {
    world: falcon_simulation::World,
}

#[derive(Serialize)]
struct Samples {
    calls: usize,
    total_us: f64,
    mean_us: f64,
    median_us: f64,
    p95_us: f64,
    max_us: f64,
}

fn summarize(mut values: Vec<u64>) -> Samples {
    assert!(!values.is_empty());
    values.sort_unstable();
    let calls = values.len();
    let total_us = values.iter().map(|v| *v as f64).sum::<f64>() / 1000.0;
    Samples {
        calls,
        total_us,
        mean_us: total_us / calls as f64,
        median_us: values[calls.div_ceil(2) - 1] as f64 / 1000.0,
        p95_us: values[(95 * calls).div_ceil(100) - 1] as f64 / 1000.0,
        max_us: values[calls - 1] as f64 / 1000.0,
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!cfg!(debug_assertions), "use cargo build --release");
    // This mode must be invoked before the CLI installs its subscriber.
    assert!(!tracing::enabled!(tracing::Level::ERROR));
    let actions = fixture::baseline::load()?;
    let golden: Vec<[Recorded; 2]> =
        serde_json::from_slice(include_bytes!("17_launch_trace.json"))?;
    assert_eq!(golden.len(), 180);
    let mut ordinary = Vec::with_capacity(1790);
    let mut delivery = Vec::with_capacity(10);
    let mut runs = Vec::with_capacity(10);
    let mut verification = Vec::with_capacity(10);
    let mut raw = Vec::with_capacity(10);
    for trial in 0..13 {
        let mut runtime = Runtime::new(&actions, true, true)?;
        let mut displays = Vec::with_capacity(180);
        let mut elapsed_ns = Vec::with_capacity(180);
        for tick in 0..180 {
            let bits = falcon_simulation::fixture_input(tick);
            let start = Instant::now();
            let pair = black_box(runtime.advance(black_box(bits))?);
            let elapsed = start.elapsed().as_nanos() as u64;
            elapsed_ns.push(elapsed);
            displays.push(pair);
        }
        let check = Instant::now();
        for (pair, expected) in displays.iter().zip(&golden) {
            for (actual, old) in pair.iter().zip(expected) {
                assert_eq!(actual.world, old.world);
            }
        }
        assert_eq!(runtime.tick(), 180);
        assert!(!displays[97][1].restored.is_empty());
        assert_eq!(displays[97][1].advances, 20);
        let check_ns = check.elapsed().as_nanos() as u64;
        if trial >= 3 {
            verification.push(check_ns);
            runs.push(elapsed_ns.iter().sum());
            for (tick, elapsed) in elapsed_ns.iter().copied().enumerate() {
                if tick == 97 {
                    delivery.push(elapsed);
                } else {
                    ordinary.push(elapsed);
                }
            }
            raw.push(elapsed_ns);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "profile": "release", "subscriber": "none", "warmup_runs": 3,
            "measured_runs": 10, "ticks_per_run": 180, "peers": 2,
            "verified_worlds_including_warmup": 4680,
            "ordinary_tick": summarize(ordinary),
            "delivery_tick_97": summarize(delivery),
            "sum_of_180_advance_calls": summarize(runs),
            "separate_full_state_verification": summarize(verification),
            "raw_tick_nanoseconds_by_run": raw,
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn nearest_rank_percentiles_and_units() {
        assert_eq!(
            serde_json::to_value(super::summarize(vec![4000, 1000, 3000, 2000])).unwrap(),
            serde_json::json!({"calls":4,"total_us":10.0,"mean_us":2.5,
                "median_us":2.0,"p95_us":4.0,"max_us":4.0})
        );
    }
}
