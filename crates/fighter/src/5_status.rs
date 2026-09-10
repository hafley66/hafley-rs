//! Finite executable transition inventory for status export.
//!
//! Every entry is obtained by evaluating the public decision functions
//! ([`crate::ground::decide`], [`crate::air::decide`]) over the full boolean
//! fact inventory. This module owns no transition specification of its own: it
//! varies inputs and records the observed outputs, so a chart change is visible
//! here without editing it.

use crate::{Phase, air, ground};

/// One observed `(source phase, event, destination)` triple with the number of
/// fact assignments that produce it. `to == None` is an explicit rejection that
/// preserves the source phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub from: Phase,
    pub event: &'static str,
    pub to: Option<Phase>,
    pub witnesses: u32,
}

fn ground_facts(bits: u8) -> ground::Facts {
    ground::Facts {
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
    event: &'static str,
    to: Option<Phase>,
) {
    if let Some(entry) = out.iter_mut().find(|t| t.from == from && t.event == event && t.to == to) {
        entry.witnesses += 1;
    } else {
        out.push(Transition { from, event, to, witnesses: 1 });
    }
}

/// Every ground decision over `Phase::ALL` x {JumpRequest, GroundIntent, Motion}.
pub fn ground_transitions() -> Vec<Transition> {
    let mut out = Vec::new();
    for from in Phase::ALL {
        record(&mut out, from, "JumpRequest", ground::decide(from, ground::Event::JumpRequest));
        for bits in 0..=127u8 {
            record(
                &mut out,
                from,
                "GroundIntent",
                ground::decide(from, ground::Event::GroundIntent(ground_facts(bits))),
            );
            record(
                &mut out,
                from,
                "Motion",
                ground::decide(from, ground::Event::Motion(ground_facts(bits))),
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
            let facts = air::AirFacts {
                descending: bits & 1 != 0,
                jump_pressed: bits & 2 != 0,
                jumps_left: u8::from(bits & 4 != 0),
            };
            record(&mut out, from, "Motion", air::decide(from, air::AirEvent::Motion(facts)));
        }
        record(&mut out, from, "Land", air::decide(from, air::AirEvent::Land));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(Phase::ALL.iter().all(|phase| transitions.iter().any(|t| t.from == *phase)));
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
                transitions.iter().any(|t| t.to == Some(to) && t.from == from),
                "missing {from:?} -> {to:?}"
            );
        }
        assert!(air_transitions().iter().any(|t| t.event == "Land" && t.to.is_none()));
    }
}
