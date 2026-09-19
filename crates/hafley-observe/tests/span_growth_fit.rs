use bigoish::{assert_best_fit, N};

fn nested_span_workload(rows: usize) -> usize {
    let populate = tracing::debug_span!("populate", rows = rows);
    let _populate_guard = populate.enter();
    let mut visits = 0usize;
    for row in 0..rows {
        let maintain = tracing::debug_span!("maintain", row = row);
        let _maintain_guard = maintain.enter();
        visits += 1;
    }
    visits
}

#[test]
fn span_visit_growth_fits_linear() {
    for rows in [100usize, 1000, 10000] {
        assert_eq!(nested_span_workload(rows), rows, "child count under populate must equal rows");
    }

    let sizes = [100usize, 1000, 10000];
    assert_best_fit(N, nested_span_workload, sizes.map(|rows| (rows, rows)));
}
