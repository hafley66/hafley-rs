//! The TypeScript cleave oracle cases over real corpora: type/interface
//! declarations move with their imports, default/namespace/type imports
//! survive both arms, diagnostics and destination collisions stop before
//! writing, overloads move with their exports and recompiles under tsc, and
//! source/destination import accounting stays exact. Every original
//! contains/absence/file-equality/tsc claim is a step in
//! `tests/fixtures/cleave_ts_oracle_outputs/`.

#[test]
fn whole_output() {
    crate::fixture_runner::run("cleave_ts_oracle_outputs", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
