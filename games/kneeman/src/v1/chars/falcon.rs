//! Roster row 1: the Falcon row (queue-2026-07-03 item 4), moved out of tune.rs so
//! "add a character" means "add a file" (plans/swordsman-lucas.md row 1).

use crate::v1::chars::kneeman;
use crate::v1::{CharSpec, Hitbox, SpecialMove};

/// KneeMan's kit with DiveGrab up-special and jump-restoring down-special.
/// Kick uses separate ground/air damage phases with authored timing, geometry and angle-361 approximation.
pub fn spec() -> CharSpec {
    let mut s = kneeman::spec();
    // PM3.6 AttackAirF combat values and displayed active windows mapped to f.frame.
    // Existing Game3 shapes remain authored; electric effects/landing windows are unported.
    let shapes = [s.fair.boxes[0], s.fair.boxes[1]];
    s.fair.startup = 14;
    s.fair.recovery = 5; // final active frame 30, interrupt threshold 36
    s.fair.nbox = 4;
    for (index, hit) in s.fair.boxes.iter_mut().enumerate() {
        let early = index < 2;
        *hit = Hitbox {
            id: 0, // one victim hit across both spatial shapes and temporal phases
            start: if early { 14 } else { 17 }, len: if early { 3 } else { 14 },
            damage: if early { 18.0 } else { 6.0 }, angle: if early { 32.0 } else { 361.0 },
            bkb: if early { 24.0 } else { 35.0 }, kbg: if early { 100.0 } else { 80.0 },
            ..shapes[index % 2]
        };
    }
    s.specials[2] = SpecialMove::FALCON_DIVE;
    s.specials[3] = SpecialMove::FALCON_KICK;
    s
}

#[cfg(test)]
#[test]
fn fair_phases_are_character_local_and_share_hit_identity() {
    let falcon = spec().fair;
    let original = kneeman::spec().fair;
    assert_eq!((original.startup, original.total(), original.boxes[0].damage), (7, 36, 16.0));
    assert_eq!((falcon.startup, falcon.total(), falcon.nbox), (14, 36, 4));
    let rows: Vec<_> = falcon.live_boxes().iter().map(|h|
        (h.id, h.start, h.len, h.damage, h.angle, h.bkb, h.kbg, h.refresh)).collect();
    assert_eq!(rows, vec![
        (0,14,3,18.0,32.0,24.0,100.0,0), (0,14,3,18.0,32.0,24.0,100.0,0),
        (0,17,14,6.0,361.0,35.0,80.0,0), (0,17,14,6.0,361.0,35.0,80.0,0),
    ]);
    for (frame, damage) in [(13,None),(14,Some(18.0)),(16,Some(18.0)),
        (17,Some(6.0)),(30,Some(6.0)),(31,None)] {
        assert_eq!(falcon.box_at(frame).map(|h|h.damage),damage);
    }
}

#[cfg(test)]
#[test]
fn fair_early_and_late_contacts_replay_without_a_second_hit() {
    use crate::v1::{InputFrame, SimState, Tune, net, step};
    let tune = Tune::default();
    for (late, expected) in [(false,18.0),(true,6.0)] {
        let mut state = crate::fixtures::fair_contact(late);
        let mut restored: SimState = bincode::deserialize(&bincode::serialize(&state).unwrap()).unwrap();
        for _ in 0..60 {
            state = step(&state, &[&InputFrame::default(); 2], &tune);
            restored = step(&restored, &[&InputFrame::default(); 2], &tune);
            assert_eq!(net::checksum(&state), net::checksum(&restored));
            assert_eq!(state.fighters[1].damage, expected);
        }
    }
}
