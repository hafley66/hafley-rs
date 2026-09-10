//! Deterministic static rendering of the live grounded decision function.
//!
//! The renderer is deliberately driven by [`crate::ground::decide`]. It owns
//! only the finite inventory needed to inspect the public Phase/Event values
//! and a small exact guard summarizer for compressing equivalent exhaustive
//! assignments. It does not contain a second transition specification.

use crate::{Phase, ground};
use ground::{Event, Facts};

const FACT_NAMES: [&str; 7] = [
    "dash", "walk", "forward", "reverse", "down", "finished", "stopped",
];

const PHASES: [Phase; 12] = [
    Phase::Idle,
    Phase::Walk,
    Phase::Dash,
    Phase::Run,
    Phase::Brake,
    Phase::Turn,
    Phase::Squat,
    Phase::Crouch,
    Phase::Landing,
    Phase::Jump,
    Phase::Fall,
    Phase::AirJump,
];

const EVENTS: [&str; 3] = ["JumpRequest", "GroundIntent", "Motion"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cube([u8; 7]);

impl Cube {
    fn from_assignment(bits: u8) -> Self {
        let mut values = [0; 7];
        for (index, value) in values.iter_mut().enumerate() {
            *value = bits >> index & 1;
        }
        Self(values)
    }

    fn label(self) -> String {
        let terms: Vec<_> = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                0 => Some(format!("!{}", FACT_NAMES[index])),
                1 => Some(FACT_NAMES[index].to_string()),
                _ => None,
            })
            .collect();
        if terms.is_empty() {
            "true".to_string()
        } else {
            terms.join(" && ")
        }
    }
}

fn facts(bits: u8) -> Facts {
    Facts {
        dash: bits & 1 << 0 != 0,
        walk: bits & 1 << 1 != 0,
        forward: bits & 1 << 2 != 0,
        reverse: bits & 1 << 3 != 0,
        down: bits & 1 << 4 != 0,
        finished: bits & 1 << 5 != 0,
        stopped: bits & 1 << 6 != 0,
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "Idle",
        Phase::Walk => "Walk",
        Phase::Dash => "Dash",
        Phase::Run => "Run",
        Phase::Brake => "Brake",
        Phase::Turn => "Turn",
        Phase::Squat => "Squat",
        Phase::Crouch => "Crouch",
        Phase::Landing => "Landing",
        Phase::Jump => "Jump",
        Phase::Fall => "Fall",
        Phase::AirJump => "AirJump",
    }
}

fn event_result(phase: Phase, event_index: usize, bits: u8) -> Option<Phase> {
    match event_index {
        0 => ground::decide(phase, Event::JumpRequest),
        1 => ground::decide(phase, Event::GroundIntent(facts(bits))),
        2 => ground::decide(phase, Event::Motion(facts(bits))),
        _ => unreachable!("the event inventory is fixed above"),
    }
}

fn assignments_for(phase: Phase, event_index: usize, destination: Option<Phase>) -> Vec<u8> {
    (0..128u8)
        .filter(|bits| event_result(phase, event_index, *bits) == destination)
        .collect()
}

fn cube_matches(cube: Cube, bits: u8) -> bool {
    cube.0
        .iter()
        .enumerate()
        .all(|(index, value)| *value == 2 || *value == (bits >> index & 1))
}

/// Summarize a result group only when its common facts exactly describe the
/// group. This is a direct exhaustive verification, rather than a transition
/// or boolean-minimization specification.
fn exact_cubes(phase: Phase, event_index: usize, destination: Option<Phase>) -> Vec<(Cube, usize)> {
    let assignments = assignments_for(phase, event_index, destination);
    if assignments.is_empty() {
        return Vec::new();
    }
    let mut values = [2; 7];
    for (index, value) in values.iter_mut().enumerate() {
        let first = assignments[0] >> index & 1;
        if assignments.iter().all(|bits| bits >> index & 1 == first) {
            *value = first;
        }
    }
    let common = Cube(values);
    let covered: Vec<_> = (0..128u8)
        .filter(|bits| cube_matches(common, *bits))
        .collect();
    if covered
        .iter()
        .all(|bits| event_result(phase, event_index, *bits) == destination)
    {
        return vec![(common, assignments.len())];
    }
    assignments
        .into_iter()
        .map(|bits| (Cube::from_assignment(bits), 1))
        .collect()
}

fn event_label(event_index: usize, cube: Cube, witnesses: usize) -> String {
    let event = EVENTS[event_index];
    if event_index == 0 {
        format!("{event} ({witnesses}/128 facts)")
    } else {
        format!("{event} [{}] ({witnesses}/128 facts)", cube.label())
    }
}

fn destination_name(destination: Option<Phase>) -> &'static str {
    destination.map(phase_name).unwrap_or("Handled")
}

fn render_edge(
    phase: Phase,
    event_index: usize,
    destination: Option<Phase>,
    cube: Cube,
    witnesses: usize,
) -> String {
    let label = event_label(event_index, cube, witnesses);
    if destination.is_none() {
        format!("    {} -.\"{}\".-> Handled\n", phase_name(phase), label,)
    } else {
        format!(
            "    {} --> {}: {}\n",
            phase_name(phase),
            destination_name(destination),
            label,
        )
    }
}

/// Render the current grounded decision function as a source-backed Mermaid
/// statechart. Every event/phase/fact assignment is evaluated by `decide`.
pub fn render() -> String {
    let mut output = String::from(
        "# Ground statechart (generated)\n\n\
Source: `src/1b_ground.rs`, rendered from `ground::decide(Phase, Event)`.\n\
Inventory: 12 phases × 3 event variants × 128 assignments of the seven `Facts`\n\
booleans. `JumpRequest` is evaluated across all assignments to verify that it\n\
is independent of facts.\n\
\n\
`Handled` means `decide` returned `None`: the phase and its clock are preserved.\n\
Solid edges are returned `Some(destination)` decisions. A self-edge is a real\n\
self-transition, including the dash-age reset documented in `1b_ground.rs`.\n\
Witness counts show the exhaustive assignments represented by each edge.\n\
This chart covers the current grounded local policy. Physics integration,\n\
air/contact behavior and the broader source inventory remain outside this chart.\n\n\
```mermaid\nstateDiagram-v2\ndirection LR\n",
    );
    output.push_str("    Handled: handled / no transition\n");
    for phase in PHASES {
        output.push_str(&format!("    {}\n", phase_name(phase)));
    }

    for phase in PHASES {
        for event_index in 0..EVENTS.len() {
            for destination in PHASES.into_iter().map(Some).chain([None]) {
                for (cube, witnesses) in exact_cubes(phase, event_index, destination) {
                    output.push_str(&render_edge(
                        phase,
                        event_index,
                        destination,
                        cube,
                        witnesses,
                    ));
                }
            }
        }
    }
    output.push_str("```\n");
    output
}
