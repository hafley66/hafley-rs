//! Enum-state machine declaration, composed routing, ordered broadcast, and
//! D2/SVG export from the same declared rows.
//!
//! Run without arguments to print a deterministic routing trace. Run with
//! `--write` to emit the D2 documents next to this example, normalise them with
//! `d2 fmt`, gate them with `d2 --check`, and render SVG with the installed
//! `d2 --layout elk`. No transition graph is handwritten and no Rust source is
//! parsed: the graph metadata comes from the `machine!` rows in `model.rs`.

use std::{env, fs, path::PathBuf, process::Command};

#[path = "5_machine_enum/model.rs"]
mod model;

use model::*;

fn composed_d2() -> String {
    let mut out = String::from(
        "direction: right\nclasses: {\n  state: {style: {fill: \"#18303b\"; stroke: \"#65d9e6\"; font-color: \"#e2edf0\"; border-radius: 8}}\n  transition: {style: {stroke: \"#65d9e6\"; font-color: \"#efcf75\"}}\n  route: {style: {stroke: \"#efcf75\"; stroke-dash: 4; font-color: \"#efcf75\"}}\n}\n\"Composed routing (illustrative cross edges)\": \"Composed routing (illustrative cross edges)\" {\n",
    );
    render_d2_prefixed(&mut out, &SESSION_GRAPH, "  ", "S_");
    render_d2_prefixed(&mut out, &GATE_GRAPH, "  ", "G_");
    for outcome in ["Handled", "Unhandled"] {
        out.push_str(&format!(
            "  \"O_{outcome}\": \"{outcome}\" {{ class: state }}\n"
        ));
    }
    for route in ROUTES {
        // Cross edges reflect the authored router table, not row-derived
        // reachability, and are labelled accordingly.
        out.push_str(&format!(
            "  \"O_{outcome}\" -> \"G_{target}\": \"{event} (illustrative)\" {{ class: route }}\n",
            outcome = route.outcome,
            target = route.gate_target,
            event = route.gate_event,
        ));
    }
    out.push_str("}\n");
    out
}

fn run_d2(args: &[&str]) -> Result<(), String> {
    let status = Command::new("d2")
        .args(args)
        .status()
        .map_err(|err| format!("failed to launch d2: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("d2 {args:?} exited with {status}"))
    }
}

fn print_trace() {
    let cx = Cx { max_attempts: 2 };
    let session_tape = [
        SessionEvent::Begin { id: 11 },
        SessionEvent::Retry,
        SessionEvent::Confirm { token: 7 },
        SessionEvent::Tick,
        SessionEvent::Unknown,
    ];
    let (session, outs, fx) = run_session(&session_tape, cx);
    println!("session state={session:?} outcomes={outs:?} fx={fx:?}");

    let mut system = System::default();
    let tape = [
        SessionEvent::Begin { id: 11 },
        SessionEvent::Retry,
        SessionEvent::Confirm { token: 7 },
        SessionEvent::Tick,
        SessionEvent::Tick,
        SessionEvent::Unknown,
        SessionEvent::Abort,
    ];
    for ev in tape {
        let mut fx = Vec::new();
        let outcome = step_routed(&mut system, ev.clone(), cx, &mut fx);
        println!(
            "routed {ev:?} -> {outcome:?} gate={:?} fx={fx:?}",
            system.gate
        );
    }

    let mut ledger = Ledger::default();
    step_broadcast(
        &mut ledger,
        Bulletin {
            topic: "release".into(),
            body: "1.2.0".into(),
        },
    );
    println!("broadcast ledger={:?}", ledger.log);
}

fn main() {
    print_trace();

    if !env::args().any(|argument| argument == "--write") {
        return;
    }

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let single = dir.join("5_machine_enum.d2");
    let composed = dir.join("5_machine_enum_composed.d2");
    fs::write(&single, machine_d2(&SESSION_GRAPH, "Session machine")).expect("write single D2");
    fs::write(&composed, composed_d2()).expect("write composed D2");

    for (d2_path, svg_path) in [
        (&single, dir.join("5_machine_enum.svg")),
        (&composed, dir.join("5_machine_enum_composed.svg")),
    ] {
        let path = d2_path.to_str().expect("utf-8 path");
        let svg = svg_path.to_str().expect("utf-8 path");
        run_d2(&["fmt", path]).expect("d2 fmt");
        run_d2(&["--check", path]).expect("d2 --check");
        run_d2(&["--layout", "elk", path, svg]).expect("d2 render");
        println!("wrote {path} and {svg}");
    }
}
