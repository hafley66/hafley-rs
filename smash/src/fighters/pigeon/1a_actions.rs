//! Lab transition policy over imported PM timing/flag/pose data.
//! This does not execute Brawl common callbacks or claim PM physics equivalence.
use super::{Action, World};
use game_content::Trigger;

#[path = "generated/0_chart.rs"]
mod generated;

const IDLE: usize = 0;
const JUMP: usize = 1;
const FAIR: usize = 2;
const SQUAT: usize = 3;
const FALL: usize = 4;
const LAND_FAIR: usize = 5;
const LAND: usize = 6;

fn target(from: usize, trigger: Trigger) -> usize {
    generated::TRANSITIONS.iter()
        .find(|transition| transition.from == from && transition.trigger == trigger)
        .unwrap_or_else(|| panic!("missing transition from {from} for {trigger:?}"))
        .to
}

fn enter(world: &mut World, action: usize) {
    let fighter = world.movement.as_mut().expect("canonical Pigeon State");
    tracing::debug!(target: "pigeon::transition", tick = world.frame, from = fighter.action.id, to = action);
    fighter.action.id = action;
    fighter.action.frame = 0;
    if action == FAIR {
        world.attack_hit = false;
    }
}

pub fn advance(world: &mut World, pressed: u8, actions: &[Action], axis: f32) -> [f32; 3] {
    let action = world.movement.as_ref().expect("canonical Pigeon State").action.id;
    let animation = world.movement.as_ref().expect("canonical Pigeon State").action.frame;
    // Finish the previously displayed action before accepting this tick's input.
    if animation >= actions[action].frames.len() {
        match action {
            SQUAT => {
                enter(world, target(SQUAT, Trigger::Complete));
                world.jump_at = Some(world.frame);
            }
            JUMP | FAIR => enter(world, target(action, Trigger::Complete)),
            LAND_FAIR | LAND => enter(world, target(action, Trigger::Complete)),
            IDLE | FALL => world.movement.as_mut().expect("canonical Pigeon State").action.frame = 0,
            _ => unreachable!(),
        }
    }
    if world.movement.as_ref().expect("canonical Pigeon State").action.id == IDLE && pressed & 1 != 0 {
        enter(world, target(IDLE, Trigger::JumpPress));
    }
    let air = world.jump_at.map_or(0.0, |t| (world.frame - t) as f32);
    let height = (0.95 * air - 0.018 * air * air).max(0.0);
    // Ground contact takes precedence over an aerial input on the same tick.
    if world.jump_at.is_some() && air > 0.0 && height == 0.0 {
        let action = world.movement.as_ref().expect("canonical Pigeon State").action.id;
        let animation = world.movement.as_ref().expect("canonical Pigeon State").action.frame;
        let action_source = &actions[action];
        let frame = &action_source.frames[animation.min(action_source.frames.len() - 1)];
        let landing = if action == FAIR && frame.landing_lag {
            target(FAIR, Trigger::LandDuringAttack)
        } else {
            target(action, Trigger::Land)
        };
        enter(world, landing);
        world.jump_at = None;
    }
    let action = world.movement.as_ref().expect("canonical Pigeon State").action.id;
    let animation = world.movement.as_ref().expect("canonical Pigeon State").action.frame;
    let source = &actions[action];
    let interruptible = source.frames[animation.min(source.frames.len() - 1)].interruptible;
    let eligible = height > 0.0
        && (matches!(action, JUMP | FALL) || (action == FAIR && interruptible));
    let attack = if let Some(buffer) = &mut world.input_buffer {
        let out = buffer
            .state
            .advance(pressed & 2 != 0, eligible, buffer.cancel, buffer.window);
        buffer.cancel = false;
        buffer.consumed = out == game_input::Outcome::Consumed;
        buffer.expired = out == game_input::Outcome::Expired;
        buffer.cancelled = out == game_input::Outcome::Cancelled;
        tracing::trace!(target: "pigeon::input", tick = world.frame,
            window = buffer.window, pressed = pressed & 2 != 0, eligible,
            pending = buffer.state.remaining().is_some(), remaining = buffer.state.remaining(),
            consumed = buffer.consumed, expired = buffer.expired, cancelled = buffer.cancelled);
        buffer.consumed
    } else {
        eligible && pressed & 2 != 0
    };
    if attack {
        enter(world, target(action, Trigger::AttackEligible));
    }
    [
        0.0,
        height,
        (if world.frame == 0 {
            -12.0
        } else {
            world.view.root[2]
        }) + axis * 0.9,
    ]
}
