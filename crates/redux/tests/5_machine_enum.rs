//! Integration tests for the enum-state `machine!` declaration, graph metadata,
//! composed routing, ordered broadcast, and D2 export.

use std::{fs, process::Command};

#[path = "../examples/5_machine_enum/model.rs"]
mod model;

use model::*;

const CX: Cx = Cx { max_attempts: 2 };

// ---------------------------------------------------------------------------
// Graph metadata from the same rows.
// ---------------------------------------------------------------------------

#[test]
fn session_graph_validates_with_an_isolated_node() {
    SESSION_GRAPH
        .validate()
        .expect("session graph declares unique ids and endpoints");
    assert!(
        SESSION_GRAPH.nodes.iter().any(|node| node.id == "Faulted"),
        "the isolated node is declared"
    );
    assert!(
        SESSION_GRAPH
            .edges
            .iter()
            .all(|edge| edge.source != "Faulted" && edge.target != "Faulted"),
        "Faulted has no declared endpoints"
    );
    assert_eq!(SESSION_GRAPH.nodes.len(), 4);
    assert_eq!(SESSION_GRAPH.edges.len(), 8);
}

#[test]
fn gate_graph_validates_with_an_isolated_node() {
    GATE_GRAPH
        .validate()
        .expect("gate graph declares unique ids and endpoints");
    assert!(GATE_GRAPH.nodes.iter().any(|node| node.id == "Spent"));
    assert!(
        GATE_GRAPH
            .edges
            .iter()
            .all(|edge| edge.source != "Spent" && edge.target != "Spent")
    );
}

#[test]
fn edge_labels_and_stay_targets_come_from_the_rows() {
    let confirm = SESSION_GRAPH
        .edges
        .iter()
        .find(|edge| edge.id == "CONFIRM")
        .expect("CONFIRM row");
    assert_eq!(confirm.source, "Awaiting");
    assert_eq!(confirm.event, "Confirm");
    assert_eq!(confirm.guard, None);
    assert_eq!(confirm.action, Some("confirm"));
    assert_eq!(confirm.target, "Open");

    let retry = SESSION_GRAPH
        .edges
        .iter()
        .find(|edge| edge.id == "RETRY")
        .expect("RETRY row");
    assert_eq!(retry.guard, Some("can_retry"));
    assert_eq!(
        retry.target, "Awaiting",
        "explicit stay maps to the source node"
    );

    let fallback = SESSION_GRAPH
        .edges
        .iter()
        .find(|edge| edge.id == "IDLE_PING_FALLBACK")
        .expect("fallback row");
    assert_eq!(fallback.guard, None);
    assert_eq!(fallback.target, "Idle", "stay self edge");
}

#[test]
fn d2_edges_carry_the_stable_row_id() {
    let doc = machine_d2(&SESSION_GRAPH, "Session machine");
    assert!(
        doc.contains("IDLE_PING_GUARDED: Ping [never] ping()"),
        "{doc}"
    );
    assert!(doc.contains("IDLE_PING_FALLBACK: Ping ping()"), "{doc}");
    assert!(doc.contains("CONFIRM: Confirm"), "{doc}");
}

// ---------------------------------------------------------------------------
// Payload transitions, stay counters, guard fallback, action ordering.
// ---------------------------------------------------------------------------

#[test]
fn typed_payload_transition_constructs_the_target_from_the_event() {
    let mut st = SessionState::Idle;
    let mut fx = Vec::new();
    assert_eq!(
        step_session(&mut st, SessionEvent::Begin { id: 3 }, CX, &mut fx),
        SessionOutcome::Handled
    );
    assert_eq!(st, SessionState::Awaiting { attempts: 1 });
    assert_eq!(fx, vec![Signal::Notify { id: 3 }]);

    fx.clear();
    assert_eq!(
        step_session(&mut st, SessionEvent::Confirm { token: 9 }, CX, &mut fx),
        SessionOutcome::Handled
    );
    assert_eq!(st, SessionState::Open { token: 9, frame: 0 });
    assert_eq!(
        fx,
        vec![Signal::Notify { id: 1 }, Signal::Opened { token: 9 }]
    );
}

#[test]
fn frame_counter_stay_preserves_the_payload() {
    let mut st = SessionState::Open { token: 5, frame: 0 };
    let mut fx = Vec::new();
    for _ in 0..3 {
        assert_eq!(
            step_session(&mut st, SessionEvent::Tick, CX, &mut fx),
            SessionOutcome::Handled
        );
    }
    assert_eq!(st, SessionState::Open { token: 5, frame: 3 });
    assert!(fx.is_empty(), "Tick emits no effect");
}

#[test]
fn guard_failure_falls_through_to_the_lower_priority_row() {
    // IDLE_PING_GUARDED has `never`; the second Ping row handles it.
    let mut st = SessionState::Idle;
    let mut fx = Vec::new();
    assert_eq!(
        step_session(&mut st, SessionEvent::Ping, CX, &mut fx),
        SessionOutcome::Handled
    );
    assert_eq!(fx, vec![Signal::Warn]);
    assert_eq!(st, SessionState::Idle, "handled stay keeps the variant");
}

#[test]
fn action_runs_before_the_target_assignment() {
    let mut st = SessionState::Awaiting { attempts: 2 };
    let mut fx = Vec::new();
    assert_eq!(
        step_session(&mut st, SessionEvent::Confirm { token: 7 }, CX, &mut fx),
        SessionOutcome::Handled
    );
    // `confirm` observed the source variant (Notify id = attempts = 2), which is
    // only possible because the action precedes the target assignment.
    assert_eq!(
        fx,
        vec![Signal::Notify { id: 2 }, Signal::Opened { token: 7 }]
    );
    assert_eq!(st, SessionState::Open { token: 7, frame: 0 });
}

#[test]
fn guarded_retry_then_guard_rejection() {
    let mut st = SessionState::Awaiting { attempts: 1 };
    let mut fx = Vec::new();
    assert_eq!(
        step_session(&mut st, SessionEvent::Retry, CX, &mut fx),
        SessionOutcome::Handled
    );
    assert_eq!(st, SessionState::Awaiting { attempts: 2 });
    assert_eq!(
        step_session(&mut st, SessionEvent::Retry, CX, &mut fx),
        SessionOutcome::Unhandled
    );
    assert_eq!(
        st,
        SessionState::Awaiting { attempts: 2 },
        "rejected event must not move state"
    );
    assert!(fx.is_empty());
}

#[test]
fn unknown_event_is_unhandled() {
    let mut st = SessionState::Idle;
    let mut fx = Vec::new();
    assert_eq!(
        step_session(&mut st, SessionEvent::Unknown, CX, &mut fx),
        SessionOutcome::Unhandled
    );
    assert_eq!(st, SessionState::Idle);
    assert!(fx.is_empty());
}

// ---------------------------------------------------------------------------
// Composed routing and ordered broadcast.
// ---------------------------------------------------------------------------

#[test]
fn composed_routing_is_deterministic() {
    let mut system = System::default();
    let mut fx = Vec::new();
    assert_eq!(
        step_routed(&mut system, SessionEvent::Begin { id: 1 }, CX, &mut fx),
        GateOutcome::Handled
    );
    assert_eq!(system.gate, GateState::Armed { count: 0 });
    assert_eq!(fx, vec![Signal::Notify { id: 1 }, Signal::Armed]);

    fx.clear();
    assert_eq!(
        step_routed(&mut system, SessionEvent::Unknown, CX, &mut fx),
        GateOutcome::Handled
    );
    assert_eq!(system.gate, GateState::Armed { count: 0 });
    assert!(fx.is_empty(), "Unhandled routes to an inert gate event");

    fx.clear();
    let (session, outs, session_fx) = run_session(&[SessionEvent::Begin { id: 1 }], CX);
    assert_eq!(session, SessionState::Awaiting { attempts: 1 });
    assert_eq!(outs, vec![SessionOutcome::Handled]);
    assert_eq!(session_fx, vec![Signal::Notify { id: 1 }]);
}

#[test]
fn broadcast_clones_to_first_and_moves_to_last_in_order() {
    let mut ledger = Ledger::default();
    step_broadcast(
        &mut ledger,
        Bulletin {
            topic: "alpha".into(),
            body: "one".into(),
        },
    );
    assert_eq!(
        ledger.log,
        vec!["first:alpha:one".to_string(), "last:alpha:one".to_string()]
    );

    step_broadcast(
        &mut ledger,
        Bulletin {
            topic: "beta".into(),
            body: "two".into(),
        },
    );
    assert_eq!(
        ledger.log,
        vec![
            "first:alpha:one".to_string(),
            "last:alpha:one".to_string(),
            "first:beta:two".to_string(),
            "last:beta:two".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// Replay equality and snapshot restore.
// ---------------------------------------------------------------------------

fn run_routed_tape(tape: &[SessionEvent]) -> (System, Vec<GateOutcome>, Vec<Signal>) {
    let mut system = System::default();
    let mut outs = Vec::new();
    let mut fx = Vec::new();
    for ev in tape {
        outs.push(step_routed(&mut system, ev.clone(), CX, &mut fx));
    }
    (system, outs, fx)
}

#[test]
fn replay_matches_state_and_ordered_effects() {
    let tape = [
        SessionEvent::Begin { id: 4 },
        SessionEvent::Retry,
        SessionEvent::Confirm { token: 8 },
        SessionEvent::Tick,
        SessionEvent::Unknown,
        SessionEvent::Abort,
    ];
    let (system_a, outs_a, fx_a) = run_routed_tape(&tape);
    let (system_b, outs_b, fx_b) = run_routed_tape(&tape);
    assert_eq!(system_a, system_b);
    assert_eq!(outs_a, outs_b);
    assert_eq!(fx_a, fx_b);
    assert!(!fx_a.is_empty());
}

#[test]
fn serde_restore_emits_nothing_and_resumes_exactly() {
    let prefix = [SessionEvent::Begin { id: 1 }, SessionEvent::Retry];
    let suffix = [
        SessionEvent::Confirm { token: 6 },
        SessionEvent::Tick,
        SessionEvent::Abort,
    ];

    // Reference: one uninterrupted run, suffix effects recorded at the boundary.
    let mut reference = System::default();
    for ev in &prefix {
        let _ = step_routed(&mut reference, ev.clone(), CX, &mut Vec::new());
    }
    let mut fresh = Vec::new();
    for ev in &suffix {
        let _ = step_routed(&mut reference, ev.clone(), CX, &mut fresh);
    }

    // Restored: run the prefix, serialize at the boundary, deserialize, resume.
    let mut boundary = System::default();
    for ev in &prefix {
        let _ = step_routed(&mut boundary, ev.clone(), CX, &mut Vec::new());
    }
    let bytes = serde_json::to_vec(&boundary).expect("serialize prefix");
    let mut decoded: System = serde_json::from_slice(&bytes).expect("deserialize prefix");
    assert_eq!(
        decoded, boundary,
        "decode copies the value and runs no reducer"
    );
    let mut restored = Vec::new();
    for ev in &suffix {
        let _ = step_routed(&mut decoded, ev.clone(), CX, &mut restored);
    }
    assert_eq!(
        decoded, reference,
        "serde suffix run matches the reference state"
    );
    assert_eq!(
        restored, fresh,
        "ordered effects match the reference suffix"
    );

    // Broadcast ledger round-trips the same way.
    let mut ledger = Ledger::default();
    step_broadcast(
        &mut ledger,
        Bulletin {
            topic: "t".into(),
            body: "b".into(),
        },
    );
    let bytes = serde_json::to_vec(&ledger).expect("serialize ledger");
    let decoded: Ledger = serde_json::from_slice(&bytes).expect("deserialize ledger");
    assert_eq!(decoded, ledger);
}

// ---------------------------------------------------------------------------
// D2 export: real `d2` validation and SVG render.
// ---------------------------------------------------------------------------

fn d2_available() -> bool {
    Command::new("d2")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[test]
fn generated_d2_validates_and_renders_svg() {
    if !d2_available() {
        eprintln!("d2 not installed: skipping D2 validation");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let single = dir.path().join("session.d2");
    let composed = dir.path().join("composed.d2");
    fs::write(&single, machine_d2(&SESSION_GRAPH, "Session machine")).expect("write single d2");
    fs::write(&composed, composed_d2_fixture()).expect("write composed d2");

    for (d2, svg) in [
        (&single, dir.path().join("session.svg")),
        (&composed, dir.path().join("composed.svg")),
    ] {
        let status = Command::new("d2")
            .args(["fmt", d2.to_str().unwrap()])
            .status()
            .expect("run d2 fmt");
        assert!(status.success(), "d2 fmt failed for {d2:?}");
        let status = Command::new("d2")
            .args(["--check", d2.to_str().unwrap()])
            .status()
            .expect("run d2 --check");
        assert!(status.success(), "d2 --check failed for {d2:?}");
        let status = Command::new("d2")
            .args([
                "--layout",
                "elk",
                d2.to_str().unwrap(),
                svg.to_str().unwrap(),
            ])
            .status()
            .expect("run d2 render");
        assert!(status.success(), "d2 render failed for {d2:?}");
        let rendered = fs::read_to_string(&svg).expect("read svg");
        assert!(rendered.contains("<svg"), "svg output is real");
    }
}

/// Compose both machines into one document; mirrors the example's composed
/// preview builder without depending on example-local code.
fn composed_d2_fixture() -> String {
    let mut out = String::from("direction: right\n\"Composed\": \"Composed\" {\n");
    render_d2_prefixed(&mut out, &SESSION_GRAPH, "  ", "S_");
    render_d2_prefixed(&mut out, &GATE_GRAPH, "  ", "G_");
    for route in ROUTES {
        out.push_str(&format!(
            "  \"O_{outcome}\" -> \"G_{target}\": \"{event} (illustrative)\"\n",
            outcome = route.outcome,
            target = route.gate_target,
            event = route.gate_event,
        ));
    }
    out.push_str("}\n");
    out
}
