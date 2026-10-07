//! Python call-edge grind over the PyCG micro-suite: one `--resolve` run per
//! fixture file collects the whole `(caller, callee)` pair table; the
//! per-syntax claims (container slots, per-scope lambdas, call results,
//! attribute values, the param rule through bound callees, builtins, `update`
//! rebinding, MRO `__init__`, self-returns) are rows in
//! `tests/fixtures/py_call_grind_cases/0_grind.json`, asserted before the
//! snapshot freezes the tables.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("py_call_grind_cases", crate::python_call_grind_support::evaluate);
}

/// FAIL-FIRST receipt: at e08866c82 (#708) the param rule rescanned every
/// site per bound-name callee with only a per-def cycle guard, factorial in
/// the defs on the path: this ring of ten forwarding defs did not finish, and
/// `click/core.py` hung past 15 s (rc=124, 0.36 s before #708). Wall row:
/// `bench/fixtures/wall_contracts/0_rows.json`
/// (`t_118_python_call_grind::forwarding_ring_resolves_under_the_wall`).
#[test]
fn forwarding_ring_resolves_under_the_wall() {
    let started = std::time::Instant::now();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/py_call_grind/forward_ring.py");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--resolve")
        .args([path, path])
        .output()
        .expect("extract binary runs");
    assert!(out.status.success());
    let wall = started.elapsed();
    crate::wall_bench::check(
        "tests/118_python_call_grind.rs:forwarding_ring_resolves_under_the_wall",
        wall.as_secs() as f64,
        10.0,
        false,
    );
    let mut f0_root = false;
    let mut f0_f1 = false;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(fact) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if fact["record"] != "resolved_edge" {
            continue;
        }
        let caller = fact["caller_name"].as_str().unwrap_or("");
        let callee = fact["callee_name"].as_str().unwrap_or("");
        f0_root |= caller.is_empty() && callee == "f0";
        // f0(f1): f0's callback is f1, so f0 -> f1; deeper rings stay unique too.
        f0_f1 |= caller == "f0" && callee == "f1";
    }
    assert!(f0_root, "ring entry resolves: {f0_root}");
    assert!(f0_f1, "ring forward resolves: {f0_f1}");
}
