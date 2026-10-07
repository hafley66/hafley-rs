//! Recursive graph questions return the edge rows that witness each answer.
//! The committed-fixture arms (call, type, flow, slice) freeze their rows;
//! the scratch/sqlite digest door keeps its equality and closure claims as
//! live asserts in `support/32_graph_paths.rs` and freezes only the derived
//! tables.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("graph_path_cases", crate::graph_paths_support::evaluate);
}
