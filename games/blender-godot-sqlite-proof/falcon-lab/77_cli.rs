//! Command dispatch for the fixture, release measurements, and process proofs.
use crate::fixture::{baseline, render, run, sql_viewer, verify};

pub fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == crate::fixture::sql_viewer::boundary::contracts::REPEAT_FLAG) {
        baseline::telemetry::init();
        return crate::repeat::run(!std::env::args().any(|arg| arg == "--verify-only"));
    }
    if std::env::args().any(|arg| arg == "--process-peer") {
        baseline::telemetry::init();
        return crate::process_peer::run();
    }
    if std::env::args().any(|arg| arg == "--process-video") {
        return crate::process_video::run();
    }
    if std::env::args().any(|arg| arg == "--measure-release") {
        return crate::release_measure::run();
    }
    baseline::telemetry::init();
    let actions = baseline::load()?;
    let sql = std::env::args().any(|arg| arg == "--sql");
    let launch = sql || std::env::args().any(|arg| arg == "--launch");
    let trace = run(&actions, true, launch)?;
    verify(&actions, &trace)?;
    if sql {
        return sql_viewer::execute(&trace, !std::env::args().any(|arg| arg == "--verify-only"));
    }
    std::fs::write(
        if launch {
            "17_launch_trace.json"
        } else {
            "11_rollback_trace.json"
        },
        serde_json::to_vec_pretty(&trace)?,
    )?;
    if std::env::args().any(|arg| arg == "--verify-only") {
        return Ok(());
    }
    render(&actions, &trace, 0)?;
    render(&actions, &trace, 1)
}
