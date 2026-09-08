#[cfg(feature = "gdext")]
#[path = "28_extension.rs"]
mod extension;
#[path = "35_runtime.rs"]
mod fixture;
#[path = "70_release_measure.rs"]
mod release_measure;
#[path = "74_process_peer.rs"]
mod process_peer;
#[path = "76_process_video.rs"]
mod process_video;
#[path = "77_cli.rs"]
mod cli;
#[path = "78_repeat.rs"]
mod repeat;
#[path = "90_control.rs"]
mod control;
pub use control::bake_web;
pub use control::inspect_import;
pub use control::inspect_catalog;
#[path = "1c_live_rows.rs"]
mod live_rows;
#[cfg(any(test, feature = "gdext"))]
#[path = "45_schedule.rs"]
mod schedule;
pub use falcon_simulation::{Simulation, Snapshot};
pub use fixture::{Display, Runtime};
pub use cli::run_cli;
#[cfg(test)]
#[path = "37_incremental_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "64_tracing_tests.rs"]
mod tracing_tests;
