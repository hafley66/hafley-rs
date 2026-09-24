//! `init_with_sinks` with no host sinks still formats every event the filter
//! lets through; an empty sink list must not disable the subscriber.

use std::io::Write;
use std::sync::{Arc, Mutex};

use hafley_observe::{Config, OutputFormat};
use tracing_subscriber::fmt::writer::BoxMakeWriter;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn empty_sink_list_still_formats_events() {
    let captured = Captured::default();
    let writer = captured.clone();
    let config = Config {
        service_name: "empty-sinks-test",
        service_version: "0",
        default_filter: "empty_sinks=info",
        format: OutputFormat::Human,
        ansi: false,
    };
    hafley_observe::init_with_sinks(config, BoxMakeWriter::new(move || writer.clone()), Vec::new())
        .expect("init");
    tracing::info!(target: "empty_sinks", "probe-line");
    let text = String::from_utf8(captured.0.lock().expect("capture lock").clone()).expect("utf8");
    let lines: Vec<&str> = text.lines().map(|line| line.split_once("Z ").map_or(line, |(_, rest)| rest)).collect();
    assert_eq!(lines, [" INFO empty_sinks: probe-line"]);
    hafley_observe::shutdown();
}
