use hafley_observe::{assert_growth, assert_growth_sized, CountRecorder, Growth};
use tracing_subscriber::prelude::*;

fn drive_batched(rows: usize) {
    let populate = tracing::info_span!("populate", rows);
    let _entered = populate.enter();
    let batch = tracing::info_span!("maintain_batch");
    let _batch = batch.enter();
}

fn drive_per_row(rows: usize) {
    let populate = tracing::info_span!("populate", rows);
    let _entered = populate.enter();
    for row in 0..rows {
        let maintain = tracing::info_span!("maintain", row);
        let _maintain = maintain.enter();
    }
}

fn counts_for(rows: usize, drive: fn(usize)) -> hafley_observe::SpanCounts {
    let (recorder, layer) = CountRecorder::new();
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || drive(rows));
    recorder.counts()
}

#[test]
fn per_row_maintenance_reads_as_linear_fanout() {
    let small = counts_for(100, drive_per_row);
    let large = counts_for(200, drive_per_row);

    small.assert_instances("populate", 1);
    assert_eq!(small.children_of("populate", "maintain"), 100);
    assert_eq!(large.children_of("populate", "maintain"), 200);

    assert_growth(&small, &large, "maintain", 2.0, Growth::Linear);
    assert_growth(&small, &large, "populate", 2.0, Growth::Constant);
}

#[test]
fn batched_maintenance_reads_as_constant_fanout() {
    let small = counts_for(100, drive_batched);
    let large = counts_for(200, drive_batched);

    small.assert_children_at_most("populate", "maintain_batch", 1);
    assert_growth(&small, &large, "maintain_batch", 2.0, Growth::Constant);
}

fn drive_lookup(n: usize) {
    let map: std::collections::HashMap<usize, usize> = (0..n).map(|i| (i, i)).collect();
    let probe = tracing::info_span!("probe");
    let _probe = probe.enter();
    assert_eq!(map.get(&(n / 2)), Some(&(n / 2)));
}

fn drive_binary_search(n: usize) {
    let sorted: Vec<usize> = (0..n).collect();
    let (mut lo, mut hi, target) = (0, n, n - 1);
    while lo < hi {
        let probe = tracing::info_span!("probe");
        let _probe = probe.enter();
        let mid = (lo + hi) / 2;
        if sorted[mid] < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    assert_eq!(sorted[lo], target);
}

fn drive_scan(n: usize) {
    for _ in 0..n {
        let probe = tracing::info_span!("probe");
        let _probe = probe.enter();
    }
}

fn drive_all_pairs(n: usize) {
    for _ in 0..n {
        for _ in 0..n {
            let probe = tracing::info_span!("probe");
            let _probe = probe.enter();
        }
    }
}

#[test]
fn sized_growth_names_constant_log_linear_and_quadratic() {
    let cases: [(fn(usize), usize, usize, Growth); 4] = [
        (drive_lookup, 100, 10_000, Growth::Constant),
        (drive_binary_search, 100, 10_000, Growth::Log),
        (drive_scan, 100, 10_000, Growth::Linear),
        (drive_all_pairs, 10, 100, Growth::Quadratic),
    ];
    for (drive, small_n, large_n, expected) in cases {
        let small = counts_for(small_n, drive);
        let large = counts_for(large_n, drive);
        println!(
            "{expected:?}: probe entries {} at n={small_n}, {} at n={large_n}",
            small.entries_of("probe"),
            large.entries_of("probe")
        );
        assert_growth_sized(&small, &large, "probe", small_n, large_n, expected);
    }
}
