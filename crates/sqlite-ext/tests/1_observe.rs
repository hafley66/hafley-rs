use hafley_observe::CountRecorder;
use rusqlite::{types::Value, Connection, Result};
use sqlite_ext::{Collector, RowChange, Sign};
use tracing_subscriber::prelude::*;

#[test]
fn collector_batch_has_shared_observation_spans() -> Result<()> {
    let (recorder, layer) = CountRecorder::new();
    let _guard = tracing_subscriber::registry().with(layer).set_default();
    let db = Connection::open_in_memory()?;
    let mut collector = Collector::new("changes", 1);
    collector.begin();
    collector.update(
        &db,
        RowChange::new("files", Sign::Insert, vec![Value::Integer(42)]),
    )?;
    assert_eq!(collector.drain(&db)?.len(), 1);
    collector.commit();

    let counts = recorder.counts();
    assert_eq!(counts.instances["collector_update"], 1);
    assert_eq!(counts.instances["collector_drain"], 1);
    Ok(())
}
