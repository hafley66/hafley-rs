//! Cost against input size as a fitted exponent `k` in `cost ~ n^k`: a deterministic work count
//! per size, and the wall-time median of repeated runs.
use std::time::{Duration, Instant};

/// `k` with `after / before = (large_n / small_n)^k`.
pub fn fitted_exponent(before: f64, after: f64, small_n: usize, large_n: usize) -> f64 {
    if before <= 0.0 || after <= 0.0 || large_n <= small_n {
        return 0.0;
    }
    (after / before).ln() / (large_n as f64 / small_n as f64).ln()
}

/// One input size: the work count every run reported, and each run's wall time.
#[derive(Clone, Debug)]
pub struct Sample {
    pub n: usize,
    pub count: u64,
    pub runs: Vec<Duration>,
}

impl Sample {
    pub fn median(&self) -> Duration {
        let mut runs = self.runs.clone();
        runs.sort();
        runs[runs.len() / 2]
    }
}

/// Runs `work(n)` `runs` times per size, smallest first. `work` returns its deterministic
/// count; a count that differs between runs of one size fails.
pub fn sample_growth(sizes: &[usize], runs: usize, mut work: impl FnMut(usize) -> u64) -> Vec<Sample> {
    // One bench at a time per process: two benches timing in parallel skew each other's sizes.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    sizes
        .iter()
        .map(|&n| {
            let mut count = None;
            let runs = (0..runs.max(1))
                .map(|_| {
                    let started = Instant::now();
                    let measured = work(n);
                    let elapsed = started.elapsed();
                    assert!(
                        count.is_none_or(|seen| seen == measured),
                        "work count at n={n} changed between runs: {count:?} then {measured}"
                    );
                    count = Some(measured);
                    elapsed
                })
                .collect();
            Sample {
                n,
                count: count.unwrap_or_default(),
                runs,
            }
        })
        .collect()
}

/// Exponents between neighbouring sizes: `(small_n, large_n, count_k, time_k)`.
pub fn exponents(samples: &[Sample]) -> Vec<(usize, usize, f64, f64)> {
    samples
        .windows(2)
        .map(|pair| {
            let (small, large) = (&pair[0], &pair[1]);
            (
                small.n,
                large.n,
                fitted_exponent(small.count as f64, large.count as f64, small.n, large.n),
                fitted_exponent(
                    small.median().as_secs_f64(),
                    large.median().as_secs_f64(),
                    small.n,
                    large.n,
                ),
            )
        })
        .collect()
}

/// A table of every size and exponent, for the failure message and for logs.
pub fn growth_table(name: &str, samples: &[Sample]) -> String {
    let mut out = format!("{name}\nn\tcount\tmedian_ms\n");
    for sample in samples {
        out.push_str(&format!(
            "{}\t{}\t{:.1}\n",
            sample.n,
            sample.count,
            sample.median().as_secs_f64() * 1e3
        ));
    }
    out.push_str("small_n\tlarge_n\tcount_exponent\ttime_exponent\n");
    for (small, large, count, time) in exponents(samples) {
        out.push_str(&format!("{small}\t{large}\t{count:.2}\t{time:.2}\n"));
    }
    out
}

/// Fails when the largest size pair grows faster than the ceilings: the count exactly, the
/// wall-time median by its own looser ceiling, since a clock carries noise a count does not.
pub fn assert_growth_exponent(name: &str, samples: &[Sample], count_ceiling: f64, time_ceiling: f64) {
    let table = growth_table(name, samples);
    let (_, _, count, time) = *exponents(samples).last().expect("two sizes or more");
    assert!(count <= count_ceiling, "{name}: work count exponent {count:.2} > {count_ceiling}\n{table}");
    assert!(time <= time_ceiling, "{name}: wall-time exponent {time:.2} > {time_ceiling}\n{table}");
}
