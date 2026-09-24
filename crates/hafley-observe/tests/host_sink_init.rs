//! A host sink handed to `init_with_sinks` sees every event the filter lets
//! through, as rows, on the global subscriber.

use std::sync::{Arc, Mutex};

use hafley_observe::{Config, OutputFormat, Row, Sink};
use tracing_subscriber::fmt::writer::BoxMakeWriter;

#[derive(Default)]
struct Recorder {
    rows: Mutex<Vec<Row>>,
}

impl Sink for Recorder {
    fn label(&self) -> &'static str {
        "recorder"
    }

    fn write(&self, rows: &[Row]) {
        self.rows.lock().expect("recorder lock").extend_from_slice(rows);
    }
}

#[test]
fn host_sink_receives_filtered_events() {
    let recorder = Arc::new(Recorder::default());
    let config = Config {
        service_name: "host-sink-test",
        service_version: "0",
        default_filter: "host_sink=info",
        format: OutputFormat::Human,
        ansi: false,
    };
    hafley_observe::init_with_sinks(
        config,
        BoxMakeWriter::new(std::io::sink),
        vec![Arc::clone(&recorder) as Arc<dyn Sink>],
    )
    .expect("init");
    tracing::debug!(target: "host_sink", "filtered out");
    tracing::info!(target: "host_sink", on = true, pty = "p1", "recording_indicator");
    tracing::warn!(target: "other", "filtered by target");
    let landed: Vec<(&str, String, String, Vec<(String, String)>)> = recorder
        .rows
        .lock()
        .expect("recorder lock")
        .iter()
        .map(|row| (row.level, row.name.clone(), row.target.clone(), row.fields.clone()))
        .collect();
    assert_eq!(
        landed,
        vec![(
            "INFO",
            "event".to_owned(),
            "host_sink".to_owned(),
            vec![
                ("message".to_owned(), "recording_indicator".to_owned()),
                ("on".to_owned(), "true".to_owned()),
                ("pty".to_owned(), "p1".to_owned()),
            ],
        )]
    );
    hafley_observe::shutdown();
}
