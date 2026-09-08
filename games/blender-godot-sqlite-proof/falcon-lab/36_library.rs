#[cfg(feature = "gdext")]
#[path = "28_extension.rs"]
mod extension;
#[path = "35_runtime.rs"]
mod fixture;
pub use falcon_simulation::{Simulation, Snapshot};
pub use fixture::{Display, Runtime, run_cli};
#[cfg(test)]
#[path = "37_incremental_tests.rs"]
mod tests;
