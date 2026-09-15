//! One shared Grounded/Airborne machine inside the existing `redux::Slice`
//! algebra, consumed by two small models through composition only.
//!
//! # Signatures
//!
//! - `Body(pub u32)` — illustrative example-local body identity, not an
//!   existing Rapier handle.
//! - `enum LocoEvent { JumpPressed, SupportLost, Landed }` — generic semantic
//!   input, one event per dispatch.
//! - `enum LocoPhase { Grounded, Airborne }` — the one stored phase, held in
//!   `struct LocoState { phase: LocoPhase, air_jumps_remaining: u8 }`, plain
//!   serde data. Restore is an exact value copy; no hooks rerun.
//! - `struct LocoCx { body: Body, max_air_jumps: u8 }` — Copy context: body
//!   addressing and the per-consumer allowance config.
//! - `struct Intent { body: Body, impulse: Impulse }`,
//!   `enum Impulse { GroundJump, AirJump }`,
//!   `enum LocoEffect { Apply(Intent) }` — inert typed physics intent
//!   descriptors addressed to a body. `game_physics::PhysicsSlice` today
//!   consumes `Step` on a Rapier `PhysicsWorld`; it does not yet consume this
//!   `Intent`, so no integrated execution adapter is claimed. The artifact is
//!   the typed command description.
//! - `Then<GroundBase, AirJumpPolicy>` (`SharedLoco`): `GroundBase::Output =
//!   Option<LocoEvent>` — `None` means claimed, `Some(ev)` falls through.
//!   `AirJumpPolicy::Event = Option<LocoEvent>` and no-ops on `None`, so a
//!   consumed event can never re-fire in the policy layer. Both slices share
//!   `State = LocoState`, `Context = LocoCx`, `Effect = LocoEffect`.
//!   `AirJumpPolicy::Output = LocoStep { phase, unhandled: Option<LocoEvent> }`.
//! - Consumers embed via `Zoom<Lens, SharedLoco, Model>`: `Shooter` and
//!   `Fighter` each own a `LocoState`; the base handler's match arms exist once
//!   and neither consumer copies any. Differentiation is context config only.

use redux::{Lens, Slice, Then, Zoom};
use serde::{Deserialize, Serialize};

/// Illustrative example-local body identity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body(pub u32);

/// Generic semantic locomotion input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocoEvent {
    JumpPressed,
    SupportLost,
    Landed,
}

/// The one shared stored phase.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocoPhase {
    #[default]
    Grounded,
    Airborne,
}

/// Rollback state: phase plus remaining air-jump allowance.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocoState {
    pub phase: LocoPhase,
    pub air_jumps_remaining: u8,
}

/// Copy context: addressed body and per-consumer allowance config.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocoCx {
    pub body: Body,
    pub max_air_jumps: u8,
}

/// Which takeoff impulse the shared machine requests.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Impulse {
    GroundJump,
    AirJump,
}

/// Typed physics intent addressed to one body.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub body: Body,
    pub impulse: Impulse,
}

/// Inert effect descriptor; a runner outside rollback state would translate
/// this against the physics seam.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocoEffect {
    Apply(Intent),
}

/// Base-layer result: `None` = claimed, `Some(ev)` = falls through.
type Pass = Option<LocoEvent>;

/// Result of one composed dispatch.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LocoStep {
    pub phase: LocoPhase,
    pub unhandled: Pass,
}

/// Shared base claims ground jump, support loss and landing. Airborne
/// `JumpPressed` is explicitly deferred to the policy layer.
struct GroundBase;

impl Slice for GroundBase {
    type Context<'a> = LocoCx;
    type State = LocoState;
    type Event = LocoEvent;
    type Output = Pass;
    type Effect = LocoEffect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        match (st.phase, ev) {
            (LocoPhase::Grounded, LocoEvent::JumpPressed) => {
                st.phase = LocoPhase::Airborne;
                st.air_jumps_remaining = cx.max_air_jumps;
                fx(LocoEffect::Apply(Intent {
                    body: cx.body,
                    impulse: Impulse::GroundJump,
                }));
                None
            }
            (LocoPhase::Grounded, LocoEvent::SupportLost) => {
                st.phase = LocoPhase::Airborne;
                st.air_jumps_remaining = cx.max_air_jumps;
                None
            }
            (LocoPhase::Airborne, LocoEvent::Landed) => {
                st.phase = LocoPhase::Grounded;
                st.air_jumps_remaining = 0;
                None
            }
            // Grounded contact with no airborne state to settle: handled no-op.
            (LocoPhase::Grounded, LocoEvent::Landed) => None,
            // Everything else, including airborne SupportLost and airborne
            // JumpPressed, falls through explicitly.
            (_, ev) => Some(ev),
        }
    }
}

/// Policy layer: claims only what the base deferred. `None` input means the
/// base already consumed the event and this layer is a no-op.
struct AirJumpPolicy;

impl Slice for AirJumpPolicy {
    type Context<'a> = LocoCx;
    type State = LocoState;
    type Event = Pass;
    type Output = LocoStep;
    type Effect = LocoEffect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        match ev {
            None => LocoStep {
                phase: st.phase,
                unhandled: None,
            },
            Some(LocoEvent::JumpPressed)
                if st.phase == LocoPhase::Airborne && st.air_jumps_remaining > 0 =>
            {
                // Airborne jump: phase is unchanged in this hook-free model.
                st.air_jumps_remaining -= 1;
                fx(LocoEffect::Apply(Intent {
                    body: cx.body,
                    impulse: Impulse::AirJump,
                }));
                LocoStep {
                    phase: st.phase,
                    unhandled: None,
                }
            }
            Some(ev) => LocoStep {
                phase: st.phase,
                unhandled: Some(ev),
            },
        }
    }
}

/// The one shared machine, composed once for every consumer.
type SharedLoco = Then<GroundBase, AirJumpPolicy>;

/// Shooter model: loco plus its own unrelated state.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shooter {
    pub loco: LocoState,
    pub ammo: u8,
}

/// Fighter model: loco plus its own unrelated state.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fighter {
    pub loco: LocoState,
    pub damage: u8,
}

#[derive(Copy, Clone, Debug)]
struct ShooterLoco;
impl Lens<Shooter, LocoState> for ShooterLoco {
    fn get(outer: &Shooter) -> &LocoState {
        &outer.loco
    }
    fn get_mut(outer: &mut Shooter) -> &mut LocoState {
        &mut outer.loco
    }
}

#[derive(Copy, Clone, Debug)]
struct FighterLoco;
impl Lens<Fighter, LocoState> for FighterLoco {
    fn get(outer: &Fighter) -> &LocoState {
        &outer.loco
    }
    fn get_mut(outer: &mut Fighter) -> &mut LocoState {
        &mut outer.loco
    }
}

type ShooterLocoSlice = Zoom<ShooterLoco, SharedLoco, Shooter>;
type FighterLocoSlice = Zoom<FighterLoco, SharedLoco, Fighter>;

fn dispatch_shooter(st: &mut Shooter, ev: LocoEvent, cx: LocoCx) -> (LocoStep, Vec<LocoEffect>) {
    let mut fx = Vec::new();
    let out = ShooterLocoSlice::reduce(st, ev, cx, &mut |e| fx.push(e));
    (out, fx)
}

fn dispatch_fighter(st: &mut Fighter, ev: LocoEvent, cx: LocoCx) -> (LocoStep, Vec<LocoEffect>) {
    let mut fx = Vec::new();
    let out = FighterLocoSlice::reduce(st, ev, cx, &mut |e| fx.push(e));
    (out, fx)
}

fn run_tape(
    mut step: impl FnMut(LocoEvent) -> (LocoStep, Vec<LocoEffect>),
    tape: &[LocoEvent],
) -> (Vec<LocoStep>, Vec<LocoEffect>) {
    let mut steps = Vec::new();
    let mut fx = Vec::new();
    for ev in tape {
        let (out, effects) = step(*ev);
        steps.push(out);
        fx.extend(effects);
    }
    (steps, fx)
}

fn main() {
    let shooter_cx = LocoCx {
        body: Body(1),
        max_air_jumps: 0,
    };
    let fighter_cx = LocoCx {
        body: Body(2),
        max_air_jumps: 1,
    };
    let mut shooter = Shooter::default();
    let mut fighter = Fighter::default();
    for ev in [LocoEvent::JumpPressed, LocoEvent::Landed] {
        let (out, fx) = dispatch_shooter(&mut shooter, ev, shooter_cx);
        println!("shooter {ev:?}: {out:?} fx={fx:?} ammo={}", shooter.ammo);
        let (out, fx) = dispatch_fighter(&mut fighter, ev, fighter_cx);
        println!(
            "fighter {ev:?}: {out:?} fx={fx:?} damage={}",
            fighter.damage
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOOTER_CX: LocoCx = LocoCx {
        body: Body(1),
        max_air_jumps: 0,
    };
    const FIGHTER_CX: LocoCx = LocoCx {
        body: Body(2),
        max_air_jumps: 1,
    };

    fn shooter_tape(st: &mut Shooter, tape: &[LocoEvent]) -> (Vec<LocoStep>, Vec<LocoEffect>) {
        run_tape(|ev| dispatch_shooter(st, ev, SHOOTER_CX), tape)
    }

    fn fighter_tape(st: &mut Fighter, tape: &[LocoEvent]) -> (Vec<LocoStep>, Vec<LocoEffect>) {
        run_tape(|ev| dispatch_fighter(st, ev, FIGHTER_CX), tape)
    }

    #[test]
    fn ground_jump_emits_one_ground_impulse_and_preserves_full_allowance() {
        let mut fighter = Fighter::default();
        let (steps, fx) = fighter_tape(&mut fighter, &[LocoEvent::JumpPressed]);
        assert_eq!(
            fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(2),
                impulse: Impulse::GroundJump
            })],
            "exactly one GroundJump, never an AirJump in the same tick"
        );
        assert_eq!(
            steps,
            vec![LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: None
            }],
            "base claimed the event; the policy layer saw None and no-oped"
        );
        assert_eq!(
            fighter.loco.air_jumps_remaining, 1,
            "full allowance after takeoff"
        );
    }

    #[test]
    fn support_loss_and_landing_tape() {
        let mut fighter = Fighter::default();
        let (steps, fx) = fighter_tape(
            &mut fighter,
            &[
                LocoEvent::JumpPressed,
                LocoEvent::SupportLost, // airborne: must NOT refill allowance
                LocoEvent::JumpPressed, // still has the allowance from takeoff
                LocoEvent::Landed,
            ],
        );
        assert_eq!(
            fx,
            vec![
                LocoEffect::Apply(Intent {
                    body: Body(2),
                    impulse: Impulse::GroundJump
                }),
                LocoEffect::Apply(Intent {
                    body: Body(2),
                    impulse: Impulse::AirJump
                }),
            ],
            "SupportLost emitted no impulse and no refill"
        );
        assert_eq!(steps[1].unhandled, Some(LocoEvent::SupportLost));
        assert_eq!(fighter.loco.phase, LocoPhase::Grounded);
        assert_eq!(
            fighter.loco.air_jumps_remaining, 0,
            "landing resets allowance"
        );
    }

    #[test]
    fn shooter_cannot_air_jump() {
        let mut shooter = Shooter::default();
        let (steps, fx) = shooter_tape(
            &mut shooter,
            &[LocoEvent::JumpPressed, LocoEvent::JumpPressed],
        );
        assert_eq!(
            fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(1),
                impulse: Impulse::GroundJump
            })],
            "max_air_jumps = 0 leaves nothing for the policy layer to claim"
        );
        assert_eq!(
            steps[0],
            LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: None
            }
        );
        assert_eq!(
            steps[1],
            LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: Some(LocoEvent::JumpPressed)
            }
        );
        assert_eq!(shooter.loco.air_jumps_remaining, 0);
    }

    #[test]
    fn fighter_exhausts_allowance_and_repeated_support_lost_cannot_refill() {
        let mut fighter = Fighter::default();
        let (_, fx) = fighter_tape(
            &mut fighter,
            &[
                LocoEvent::JumpPressed, // takeoff, allowance 1
                LocoEvent::JumpPressed, // air jump, allowance 0
                LocoEvent::SupportLost, // airborne: unhandled, no refill
                LocoEvent::JumpPressed, // exhausted: still cannot jump
            ],
        );
        assert_eq!(fighter.loco.air_jumps_remaining, 0);
        assert_eq!(
            fx,
            vec![
                LocoEffect::Apply(Intent {
                    body: Body(2),
                    impulse: Impulse::GroundJump
                }),
                LocoEffect::Apply(Intent {
                    body: Body(2),
                    impulse: Impulse::AirJump
                }),
            ],
            "no third impulse after exhaust + repeat SupportLost"
        );
        assert_eq!(fighter.loco.phase, LocoPhase::Airborne);
    }

    #[test]
    fn shared_commands_drive_both_consumers() {
        let tape = [
            LocoEvent::JumpPressed,
            LocoEvent::Landed,
            LocoEvent::JumpPressed,
        ];
        let mut shooter = Shooter::default();
        let mut fighter = Fighter::default();
        let (shooter_steps, shooter_fx) = shooter_tape(&mut shooter, &tape);
        let (fighter_steps, fighter_fx) = fighter_tape(&mut fighter, &tape);
        // Same phase trace; different allowance and impulses by config.
        assert_eq!(
            shooter_steps.iter().map(|s| s.phase).collect::<Vec<_>>(),
            fighter_steps.iter().map(|s| s.phase).collect::<Vec<_>>()
        );
        assert_eq!(shooter_fx.len(), 2, "ground jumps only");
        assert_eq!(
            fighter_fx.len(),
            2,
            "ground jumps; the tape never goes airborne twice"
        );
        assert!(shooter_fx.iter().all(|e| e
            == &LocoEffect::Apply(Intent {
                body: Body(1),
                impulse: Impulse::GroundJump
            })));
        assert!(fighter_fx.iter().all(|e| e
            == &LocoEffect::Apply(Intent {
                body: Body(2),
                impulse: Impulse::GroundJump
            })));
    }

    #[test]
    fn unhandled_and_handled_self_cases() {
        // Handled self case: airborne jump keeps the phase and emits an effect.
        let mut fighter = Fighter::default();
        fighter_tape(&mut fighter, &[LocoEvent::JumpPressed]);
        let (steps, fx) = fighter_tape(&mut fighter, &[LocoEvent::JumpPressed]);
        assert_eq!(
            steps,
            vec![LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: None
            }]
        );
        assert_eq!(
            fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(2),
                impulse: Impulse::AirJump
            })]
        );
        // Unhandled self case: exhausted airborne jump leaves the event untouched.
        let (steps, fx) = fighter_tape(&mut fighter, &[LocoEvent::JumpPressed]);
        assert_eq!(
            steps,
            vec![LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: Some(LocoEvent::JumpPressed)
            }]
        );
        assert!(fx.is_empty());
    }

    #[test]
    fn independent_instance_ids() {
        let mut shooter = Shooter::default();
        let mut fighter = Fighter::default();
        let (shooter_steps, shooter_fx) = shooter_tape(&mut shooter, &[LocoEvent::JumpPressed]);
        assert_eq!(
            shooter_fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(1),
                impulse: Impulse::GroundJump
            })]
        );
        assert_eq!(
            fighter.loco,
            LocoState::default(),
            "untouched instance must not move"
        );
        let (fighter_steps, fighter_fx) = fighter_tape(&mut fighter, &[LocoEvent::JumpPressed]);
        assert_eq!(
            fighter_fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(2),
                impulse: Impulse::GroundJump
            })]
        );
        assert_eq!(shooter_steps[0].phase, LocoPhase::Airborne);
        assert_eq!(fighter_steps[0].phase, LocoPhase::Airborne);
        assert_eq!(
            shooter_steps[0].phase, fighter_steps[0].phase,
            "same tape, same shared phase"
        );
        assert_ne!(
            shooter.loco.air_jumps_remaining, fighter.loco.air_jumps_remaining,
            "config differs: shooter 0, fighter 1"
        );
        assert_ne!(SHOOTER_CX.body, FIGHTER_CX.body, "bodies stay distinct");
    }

    #[test]
    fn grounded_ledge_departure_tape_for_both_consumers() {
        let tape = [
            LocoEvent::SupportLost, // ledge departure from Grounded: no impulse
            LocoEvent::SupportLost, // airborne repeat: no refill
            LocoEvent::JumpPressed, // fighter still has its takeoff allowance
            LocoEvent::JumpPressed, // fighter exhausts
            LocoEvent::JumpPressed, // exhausted: cannot jump
            LocoEvent::Landed,      // reset
            LocoEvent::SupportLost, // depart again: allowance re-initialized
        ];
        let mut shooter = Shooter::default();
        let (shooter_steps, shooter_fx) = shooter_tape(&mut shooter, &tape);
        let mut fighter = Fighter::default();
        let (_, fighter_fx) = fighter_tape(&mut fighter, &tape);
        assert!(
            shooter_fx.is_empty(),
            "shooter never leaves the ground by input"
        );
        assert_eq!(
            shooter_steps[0],
            LocoStep {
                phase: LocoPhase::Airborne,
                unhandled: None
            }
        );
        assert_eq!(shooter.loco.air_jumps_remaining, 0);
        assert_eq!(
            fighter_fx,
            vec![LocoEffect::Apply(Intent {
                body: Body(2),
                impulse: Impulse::AirJump
            })],
            "no takeoff impulse; exactly one configured air jump"
        );
        assert_eq!(
            fighter.loco.air_jumps_remaining, 1,
            "re-departure re-initialized"
        );
        assert_eq!(fighter.loco.phase, LocoPhase::Airborne);
    }

    #[test]
    fn unrelated_consumer_state_is_untouched_by_the_shared_machine() {
        let mut fighter = Fighter {
            loco: LocoState::default(),
            damage: 73,
        };
        let mut shooter = Shooter {
            loco: LocoState::default(),
            ammo: 9,
        };
        fighter_tape(&mut fighter, &[LocoEvent::JumpPressed, LocoEvent::Landed]);
        shooter_tape(&mut shooter, &[LocoEvent::JumpPressed, LocoEvent::Landed]);
        assert_eq!(fighter.damage, 73);
        assert_eq!(shooter.ammo, 9);
    }

    #[test]
    fn clone_and_serde_restore_suffix_matches_fresh_execution_exactly() {
        let prefix = [LocoEvent::JumpPressed, LocoEvent::JumpPressed];
        let suffix = [
            LocoEvent::Landed,
            LocoEvent::JumpPressed,
            LocoEvent::SupportLost,
        ];
        let mut full = Vec::new();
        full.extend(prefix);
        full.extend(suffix);

        // Reference: one uninterrupted machine advancing through the prefix
        // and then the suffix, so the reference suffix is a recorded
        // prefix-boundary slice, independent of any restored run.
        let mut reference = Fighter::default();
        fighter_tape(&mut reference, &prefix);
        let (fresh_steps, fresh_fx) = fighter_tape(&mut reference, &suffix);
        assert!(!fresh_fx.is_empty(), "reference suffix must emit commands");

        // Clone restore.
        let mut original = Fighter::default();
        fighter_tape(&mut original, &prefix);
        let mut cloned = original;
        let (clone_steps, clone_fx) = fighter_tape(&mut cloned, &suffix);

        // Plain-data serde restore: an exact value copy, no hooks rerun.
        let bytes = serde_json::to_vec(&original).unwrap();
        let mut decoded: Fighter = serde_json::from_slice(&bytes).unwrap();
        let (serde_steps, serde_fx) = fighter_tape(&mut decoded, &suffix);

        assert_eq!(cloned, reference, "clone restores the full durable state");
        assert_eq!(
            decoded, reference,
            "plain-data serde restores the full durable state"
        );
        assert_eq!(
            fresh_steps, clone_steps,
            "exact vectors, not length-derived slices"
        );
        assert_eq!(fresh_steps, serde_steps);
        assert_eq!(fresh_fx, clone_fx);
        assert_eq!(fresh_fx, serde_fx);
        // Unresolved statig serde restore (entry-hook rerun on decode) is a
        // separate open question; this plain-data machine has no such seam.
    }
}
