use crate::{Simulation, fixture};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tracing_subscriber::fmt::format::FmtSpan;

#[cfg(feature = "gdext")]
#[test]
fn worker_retains_scoped_dispatch_and_parent_after_caller_returns() {
    let output = Output::default();
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(move || writer.clone())
        .finish();
    let handle = tracing::subscriber::with_default(subscriber, || {
        let _parent = tracing::info_span!("spawn_test", case = "worker_context").entered();
        crate::schedule::spawn(crate::schedule::State::default(), false)
    });
    assert_eq!(handle.join().unwrap().unwrap().len(), 180);
    let bytes = output.0.lock().unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let worker = records
        .iter()
        .find(|r| r["span"]["name"] == "worker")
        .unwrap();
    assert_eq!(worker["spans"][0]["case"], "worker_context");
    assert_eq!(worker["span"]["faults"], false);
}

#[derive(Clone, Default)]
struct Output(Arc<Mutex<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn structured_spans_record_scalar_context_without_changing_state() {
    let actions = fixture::baseline::load().unwrap();
    let baked = fixture::bake(&actions);
    let mut expected = Simulation::new(baked.clone().into(), true);
    expected.advance(1);
    let output = Output::default();
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("test_case", case = "scalar_fields");
        let _guard = span.enter();
        let mut actual = Simulation::new(baked.into(), true);
        let snapshot = actual.save();
        actual.advance(0);
        actual.load(&snapshot);
        actual.advance(1);
        assert_eq!(actual.state(), expected.state());
        let mut boundary = fixture::sql_viewer::boundary::Boundary::new().unwrap();
        let row = fixture::sql_viewer::boundary::Row::new(0, 0, 0);
        assert!(boundary.publish(&[vec![row]]));
        assert_eq!(
            fixture::sql_viewer::boundary::read_frame(&boundary.db, 0)
                .unwrap()
                .1,
            [row]
        );
        assert!(!boundary.publish(&[vec![row; 1025]]));
    });
    let bytes = output.0.lock().unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names: std::collections::BTreeSet<_> = records
        .iter()
        .filter_map(|record| record["span"]["name"].as_str())
        .collect();
    for name in [
        "save",
        "load",
        "advance_world",
        "physics_clone",
        "physics_equal",
        "publish",
        "read_frame",
    ] {
        assert!(names.contains(name), "missing {name}: {names:?}");
    }
    let advance = records
        .iter()
        .find(|r| r["span"]["name"] == "advance_world" && r["span"]["input"] == 1)
        .unwrap();
    assert_eq!(advance["span"]["tick"], 0);
    assert_eq!(advance["spans"][0]["case"], "scalar_fields");
    let refusal = records
        .iter()
        .find(|r| r["fields"]["message"] == "publication_refused")
        .unwrap();
    assert_eq!(refusal["fields"]["rows"], 1025);
    assert_eq!(refusal["fields"]["reason"], "row_capacity");
    for record in &records {
        for forbidden in ["world", "actions", "snapshot", "physics"] {
            assert!(record["span"].get(forbidden).is_none());
        }
    }
}
