#[cfg(feature = "gdext")]
#[path = "28_extension.rs"]
mod extension;
#[path = "35_runtime.rs"]
mod fixture;
#[path = "70_release_measure.rs"]
mod release_measure;
#[cfg(any(test, feature = "gdext"))]
#[path = "45_schedule.rs"]
mod schedule;
pub use falcon_simulation::{Simulation, Snapshot};
pub use fixture::{Display, Runtime, run_cli};
#[cfg(test)]
#[path = "37_incremental_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "64_tracing_tests.rs"]
mod tracing_tests;
