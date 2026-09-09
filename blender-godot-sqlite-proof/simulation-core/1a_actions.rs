//! Lab transition policy over imported PM timing/flag/pose data.
//! This does not execute Brawl common callbacks or claim PM physics equivalence.
use super::{Action, World};

const IDLE: usize = 0;
const JUMP: usize = 1;
const FAIR: usize = 2;
const SQUAT: usize = 3;
const FALL: usize = 4;
const LAND_FAIR: usize = 5;
const LAND: usize = 6;

fn enter(world: &mut World, action: usize) {
    tracing::debug!(target: "falcon::transition", tick = world.frame, from = world.action, to = action);
    world.action = action;
    world.animation = 0;
    if action == FAIR {
        world.attack_hit = false;
    }
}

pub fn advance(world: &mut World, pressed: u8, actions: &[Action], axis: f32) -> [f32; 3] {
    // Finish the previously displayed action before accepting this tick's input.
    if world.animation >= actions[world.action].frames.len() {
        match world.action {
            SQUAT => {
                enter(world, JUMP);
                world.jump_at = Some(world.frame);
            }
            JUMP | FAIR => enter(world, FALL),
            LAND_FAIR | LAND => enter(world, IDLE),
            IDLE | FALL => world.animation = 0,
            _ => unreachable!(),
        }
    }
    if world.action == IDLE && pressed & 1 != 0 {
        enter(world, SQUAT);
    }
    let air = world.jump_at.map_or(0.0, |t| (world.frame - t) as f32);
    let height = (0.95 * air - 0.018 * air * air).max(0.0);
    // Ground contact takes precedence over an aerial input on the same tick.
    if world.jump_at.is_some() && air > 0.0 && height == 0.0 {
        let action = &actions[world.action];
        let frame = &action.frames[world.animation.min(action.frames.len() - 1)];
        let landing = if world.action == FAIR && frame.landing_lag {
            LAND_FAIR
        } else {
            LAND
        };
        enter(world, landing);
        world.jump_at = None;
    }
    let source = &actions[world.action];
    let interruptible = source.frames[world.animation.min(source.frames.len() - 1)].interruptible;
    let eligible = height > 0.0
        && (matches!(world.action, JUMP | FALL) || (world.action == FAIR && interruptible));
    let attack = if let Some(buffer) = &mut world.input_buffer {
        let out = buffer
            .state
            .advance(pressed & 2 != 0, eligible, buffer.cancel, buffer.window);
        buffer.cancel = false;
        buffer.consumed = out == game_input::Outcome::Consumed;
        buffer.expired = out == game_input::Outcome::Expired;
        buffer.cancelled = out == game_input::Outcome::Cancelled;
        tracing::trace!(target: "falcon::input", tick = world.frame,
            window = buffer.window, pressed = pressed & 2 != 0, eligible,
            pending = buffer.state.remaining().is_some(), remaining = buffer.state.remaining(),
            consumed = buffer.consumed, expired = buffer.expired, cancelled = buffer.cancelled);
        buffer.consumed
    } else {
        eligible && pressed & 2 != 0
    };
    if attack {
        enter(world, FAIR);
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
