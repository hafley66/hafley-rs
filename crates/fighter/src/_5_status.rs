//! Finite executable transition inventory for status export.
//!
//! Every entry is obtained by evaluating the public decision functions
//! ([`crate::_1b_ground::decide`], [`crate::_1c_air::decide`]) over the full boolean
//! fact inventory. This module owns no transition specification of its own: it
//! varies inputs and records the observed outputs, so a chart change is visible
//! here without editing it.

use crate::{_1b_ground, _1c_air, Phase};
use serde::Serialize;

/// Runtime callback identity for one state/event dispatch. The state is kept
/// alongside the generic event name so source requirements can join this
/// inventory without guessing which handler matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CallbackIdentity {
    pub domain: &'static str,
    pub state: Phase,
    pub event: &'static str,
}

/// One observed `(source phase, event, destination)` triple with the exact
/// fact-bit assignments that produce it. `to == None` is an explicit rejection
/// that preserves the source phase. Bit positions are the public fact fields
/// in the order used by the corresponding decision function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Transition {
    pub from: Phase,
    pub event: &'static str,
    pub to: Option<Phase>,
    pub callback: CallbackIdentity,
    pub witnesses: u32,
    pub fact_bits: Vec<u8>,
}

/// Pure inventory of the executable fighter decision surface.
///
/// `states` is sourced from [`Phase::ALL`]. Ground and air entries are
/// collected by executing the public decision functions over their complete
/// finite fact cubes. The vocabulary fields are projections of those observed
/// decisions, so callers do not need a second event/effect registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeInventory {
    pub states: Vec<Phase>,
    pub ground: Vec<Transition>,
    pub air: Vec<Transition>,
    pub callbacks: Vec<CallbackIdentity>,
    pub events: Vec<&'static str>,
    pub effects: Vec<&'static str>,
}

impl RuntimeInventory {
    /// Collect the executable inventory without filesystem, clock or host IO.
    pub fn collect() -> Self {
        let ground = ground_transitions();
        let air = air_transitions();
        let mut callbacks = Vec::new();
        let mut events = Vec::new();
        let mut effects = Vec::new();
        for transition in ground.iter().chain(air.iter()) {
            if !events.contains(&transition.event) {
                events.push(transition.event);
            }
            if !callbacks.contains(&transition.callback) {
                callbacks.push(transition.callback);
            }
            let effect = match transition.to {
                None => "Handled",
                Some(to) if to == transition.from => "SelfTransition",
                Some(_) => "Transition",
            };
            if !effects.contains(&effect) {
                effects.push(effect);
            }
        }
        Self {
            states: Phase::ALL.to_vec(),
            ground,
            air,
            callbacks,
            events,
            effects,
        }
    }

    /// Alias for callers that prefer constructor terminology.
    pub fn new() -> Self {
        Self::collect()
    }
}

/// Collect the executable fighter inventory.
pub fn runtime_inventory() -> RuntimeInventory {
    RuntimeInventory::collect()
}

fn ground_facts(bits: u8) -> _1b_ground::Facts {
    _1b_ground::Facts {
        dash: bits & 1 << 0 != 0,
        walk: bits & 1 << 1 != 0,
        forward: bits & 1 << 2 != 0,
        reverse: bits & 1 << 3 != 0,
        down: bits & 1 << 4 != 0,
        finished: bits & 1 << 5 != 0,
        stopped: bits & 1 << 6 != 0,
    }
}

fn record(
    out: &mut Vec<Transition>,
    from: Phase,
    domain: &'static str,
    event: &'static str,
    to: Option<Phase>,
    fact_bits: u8,
) {
    if let Some(entry) = out
        .iter_mut()
        .find(|t| t.from == from && t.event == event && t.to == to)
    {
        entry.witnesses += 1;
        entry.fact_bits.push(fact_bits);
    } else {
        out.push(Transition {
            from,
            event,
            to,
            callback: CallbackIdentity {
                domain,
                state: from,
                event,
            },
            witnesses: 1,
            fact_bits: vec![fact_bits],
        });
    }
}

/// Every ground decision over `Phase::ALL` x {JumpRequest, GroundIntent, Motion}.
pub fn ground_transitions() -> Vec<Transition> {
    let mut out = Vec::new();
    for from in Phase::ALL {
        for bits in 0..=127u8 {
            record(
                &mut out,
                from,
                "ground",
                "JumpRequest",
                _1b_ground::decide(from, _1b_ground::Event::JumpRequest),
                bits,
            );
        }
        for bits in 0..=127u8 {
            record(
                &mut out,
                from,
                "ground",
                "GroundIntent",
                _1b_ground::decide(from, _1b_ground::Event::GroundIntent(ground_facts(bits))),
                bits,
            );
            record(
                &mut out,
                from,
                "ground",
                "Motion",
                _1b_ground::decide(from, _1b_ground::Event::Motion(ground_facts(bits))),
                bits,
            );
        }
    }
    out
}

/// Every air decision over `Phase::ALL` x {Motion(3 facts), Land}.
pub fn air_transitions() -> Vec<Transition> {
    let mut out = Vec::new();
    for from in Phase::ALL {
        for bits in 0..8u8 {
            let facts = _1c_air::AirFacts {
                descending: bits & 1 != 0,
                jump_pressed: bits & 2 != 0,
                jumps_left: u8::from(bits & 4 != 0),
            };
            record(
                &mut out,
                from,
                "air",
                "Motion",
                _1c_air::decide(from, _1c_air::AirEvent::Motion(facts)),
                bits,
            );
        }
        for bits in 0..8u8 {
            record(
                &mut out,
                from,
                "air",
                "Land",
                _1c_air::decide(from, _1c_air::AirEvent::Land),
                bits,
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transition<'a>(
        entries: &'a [Transition],
        from: Phase,
        event: &str,
        to: Option<Phase>,
    ) -> &'a Transition {
        entries
            .iter()
            .find(|entry| entry.from == from && entry.event == event && entry.to == to)
            .unwrap_or_else(|| panic!("missing {from:?} {event} -> {to:?}"))
    }

    #[test]
    fn runtime_inventory_is_sourced_from_phase_and_decision_apis() {
        let inventory = runtime_inventory();
        assert_eq!(inventory.states, Phase::ALL.to_vec());
        assert_eq!(inventory.ground, ground_transitions());
        assert_eq!(inventory.air, air_transitions());
        assert_eq!(
            inventory.events,
            ["JumpRequest", "GroundIntent", "Motion", "Land"]
        );
        assert!(inventory.callbacks.iter().any(|callback| {
            callback.domain == "ground"
                && callback.state == Phase::Dash
                && callback.event == "Motion"
        }));
        assert_eq!(
            inventory.effects,
            ["Transition", "Handled", "SelfTransition"]
        );
    }

    #[test]
    fn each_event_has_the_complete_fact_partition() {
        let inventory = runtime_inventory();
        for from in Phase::ALL {
            for event in ["JumpRequest", "GroundIntent", "Motion"] {
                let entries: Vec<_> = inventory
                    .ground
                    .iter()
                    .filter(|transition| transition.from == from && transition.event == event)
                    .collect();
                assert_eq!(
                    entries
                        .iter()
                        .map(|transition| transition.witnesses)
                        .sum::<u32>(),
                    128,
                    "{from:?} {event}"
                );
                assert_eq!(
                    entries
                        .iter()
                        .map(|transition| transition.fact_bits.len())
                        .sum::<usize>(),
                    128,
                    "{from:?} {event}"
                );
            }
        }
        for from in Phase::ALL {
            for event in ["Motion", "Land"] {
                let entries: Vec<_> = inventory
                    .air
                    .iter()
                    .filter(|transition| transition.from == from && transition.event == event)
                    .collect();
                assert_eq!(
                    entries
                        .iter()
                        .map(|transition| transition.witnesses)
                        .sum::<u32>(),
                    8,
                    "{from:?} {event}"
                );
                assert_eq!(
                    entries
                        .iter()
                        .map(|transition| transition.fact_bits.len())
                        .sum::<usize>(),
                    8,
                    "{from:?} {event}"
                );
            }
        }
    }

    #[test]
    fn runtime_inventory_keeps_self_transitions_and_guard_priority_observable() {
        let inventory = runtime_inventory();
        assert_eq!(
            transition(&inventory.ground, Phase::Dash, "Motion", Some(Phase::Dash)).witnesses,
            64
        );
        assert_eq!(
            transition(&inventory.ground, Phase::Dash, "Motion", Some(Phase::Dash)).fact_bits,
            (0..128u8)
                .filter(|bits| bits & (1 << 3) != 0)
                .collect::<Vec<_>>()
        );

        let all = _1b_ground::Facts {
            dash: true,
            walk: true,
            forward: true,
            reverse: true,
            down: true,
            finished: true,
            stopped: true,
        };
        assert_eq!(
            _1b_ground::decide(Phase::Dash, _1b_ground::Event::Motion(all)),
            Some(Phase::Dash)
        );
        assert_eq!(
            _1b_ground::decide(Phase::Run, _1b_ground::Event::Motion(all)),
            Some(Phase::Turn)
        );
        let competing_air = _1c_air::AirFacts {
            descending: true,
            jump_pressed: true,
            jumps_left: 1,
        };
        assert_eq!(
            _1c_air::decide(Phase::Jump, _1c_air::AirEvent::Motion(competing_air)),
            Some(Phase::AirJump)
        );
    }

    #[test]
    fn ground_inventory_matches_the_executable_policy() {
        let transitions = ground_transitions();
        assert!(transitions.iter().any(|t| {
            t.from == Phase::Idle && t.event == "JumpRequest" && t.to == Some(Phase::Squat)
        }));
        assert!(transitions.iter().any(|t| {
            t.from == Phase::Dash && t.event == "Motion" && t.to == Some(Phase::Dash)
        }));
        // Every phase is a source for at least one observed group, all witnessed.
        assert!(transitions.iter().all(|t| t.witnesses >= 1));
        assert!(
            Phase::ALL
                .iter()
                .all(|phase| transitions.iter().any(|t| t.from == *phase))
        );
    }

    #[test]
    fn air_inventory_covers_all_airborne_writes() {
        let transitions = air_transitions();
        for (from, to) in [
            (Phase::Jump, Phase::Fall),
            (Phase::AirJump, Phase::Fall),
            (Phase::Fall, Phase::AirJump),
            (Phase::Jump, Phase::Landing),
        ] {
            assert!(
                transitions
                    .iter()
                    .any(|t| t.to == Some(to) && t.from == from),
                "missing {from:?} -> {to:?}"
            );
        }
        assert!(
            air_transitions()
                .iter()
                .any(|t| t.event == "Land" && t.to.is_none())
        );
    }
}
