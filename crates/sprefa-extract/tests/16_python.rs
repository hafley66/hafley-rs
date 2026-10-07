//! Python front-end parity test: drives `PythonSource` directly. Expected
//! values are hand-derived from `sample.py`/`docs.py`, never copied from the
//! extractor's output; every old assert runs as code over the tables in
//! `support/28_python.rs` before the snapshot freezes them.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("python_parity_cases", crate::python_parity_support::evaluate);
}
