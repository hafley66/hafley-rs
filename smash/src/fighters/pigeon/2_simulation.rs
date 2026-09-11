use std::sync::Arc;
#[path = "0_types.rs"]
mod types;
pub use types::*;
#[path = "1b_catalog.rs"]
pub mod catalog;
#[path = "1a_actions.rs"]
mod lifecycle;
#[path = "1c_movement.rs"]
pub mod movement;
#[path = "1_sandbag.rs"]
pub mod sandbag;
use parry3d::{
    math::{Pose, Vec3},
    query,
    shape::{Ball, Cuboid},
};
use serde::{Deserialize, Serialize};
const JUMP: u8 = 1;
const ATTACK: u8 = 2;

/// Sandbag is the single target weight in this cut.
const SANDBAG_WEIGHT: f32 = 100.0;
/// The sandbag has no player input; the resolver sees a centered stick.
const NEUTRAL_STICK: [f32; 2] = [0.0, 0.0];
/// Explicit ruleset knobs for the sandbag contact path.
pub const RESOLVE_POLICY: game_combat::ResolvePolicy = game_combat::ResolvePolicy {
    hitlag_per_damage: 0.8,
    hitlag_bonus: 0,
    tumble_knockback: 80.0,
};

/// Build the strike/target pair for one connecting hitbox and resolve it once.
/// The contact path and its tests call this same seam; the outcome is fully
/// computed before anything mutates.
fn contact_outcome(
    values: &Attack,
    percent_before: f32,
    grounded: bool,
) -> game_combat::HitOutcome {
    let strike = game_combat::Strike {
        damage: values.damage,
        angle: values.trajectory,
        base_knockback: values.bkb,
        knockback_growth: values.kbg,
        weight_dependent_set_knockback: values.wdsk,
    };
    let target = game_combat::Target {
        percent: percent_before,
        weight: SANDBAG_WEIGHT,
        grounded,
    };
    game_combat::resolve_hit(
        strike,
        target,
        game_combat::DefenseInput {
            stick: NEUTRAL_STICK,
        },
        RESOLVE_POLICY,
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub movement: Option<game_fighter::State>,
    #[serde(default)]
    pub input_buffer: Option<InputBuffer>,
    pub frame: i32,
    pub jump_at: Option<i32>,
    pub attack_hit: bool,
    pub hit_count: usize,
    pub last_hit: Option<i32>,
    pub view: Tick,
    pub bag: Option<sandbag::Sandbag>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            movement: Some(game_fighter::State::new(&movement::rules())),
            input_buffer: None,
            frame: 0,
            jump_at: None,
            attack_hit: false,
            hit_count: 0,
            last_hit: None,
            view: Tick::default(),
            bag: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputBuffer {
    pub window: u32,
    pub state: game_input::Buffer,
    pub cancel: bool,
    pub consumed: bool,
    pub expired: bool,
    pub cancelled: bool,
}

pub fn fixture_input(tick: i32) -> u8 {
    match tick {
        60 => JUMP,
        78 => ATTACK,
        _ => 0,
    }
}

#[tracing::instrument(target = "pigeon::simulation", level = "trace", skip_all, fields(tick = world.frame, input = bits))]
pub fn advance_world(world: &mut World, bits: u8, actions: &[Action]) {
    advance_with_axis(world, bits, actions, None);
}

fn advance_with_axis(world: &mut World, bits: u8, actions: &[Action], axis: Option<f32>) {
    <Step as redux::Slice>::reduce(world, (bits, axis), actions, &mut |never| match never {});
}

/// The authoritative tick is the shared reducer boundary. Assets are immutable context;
/// every mutable value is in World, including input-edge history.
pub struct Step;
impl redux::Slice for Step {
    type Context<'a> = &'a [Action];
    type State = World;
    type Event = (u8, Option<f32>);
    type Output = ();
    type Effect = redux::Never;

    fn reduce(
        world: &mut World,
        (bits, axis): Self::Event,
        actions: Self::Context<'_>,
        _: &mut impl FnMut(Self::Effect),
    ) {
        reduce_tick(world, bits, actions, axis);
    }
}

fn reduce_tick(world: &mut World, bits: u8, actions: &[Action], axis: Option<f32>) {
    if let Some(bag) = &mut world.bag {
        bag.advance();
    }
    let previous_input = world
        .movement
        .as_ref()
        .map_or(0, |fighter| fighter.input_history.previous.buttons as u8);
    let pressed = bits & !previous_input;
    let imported = actions.len() >= 7;
    let live = actions.len() == catalog::ACTION_COUNT;
    if !live {
        world
            .movement
            .as_mut()
            .expect("canonical Pigeon State")
            .input_history
            .advance(game_input::PlayerInput {
                buttons: u32::from(bits),
                axes: [game_input::quantize_axis(axis.unwrap_or(0.0)), 0, 0, 0],
            });
    }
    let root = if live {
        movement::advance(world, bits, axis.unwrap_or(0.0), actions)
    } else if imported {
        lifecycle::advance(world, pressed, actions, axis.unwrap_or(0.0))
    } else {
        if pressed & JUMP != 0 && world.view.root[1] == 0.0 {
            world.jump_at = Some(world.frame);
            let fighter = world.movement.as_mut().expect("canonical Pigeon State");
            fighter.action.id = 1;
            fighter.action.frame = 0;
        }
        if pressed & ATTACK != 0 && world.jump_at.is_some() && world.view.root[1] > 0.0 {
            let fighter = world.movement.as_mut().expect("canonical Pigeon State");
            fighter.action.id = 2;
            fighter.action.frame = 0;
            world.attack_hit = false;
        }
        let action = world.movement.as_ref().expect("canonical Pigeon State").action.id;
        let air = world.jump_at.map_or(0.0, |t| (world.frame - t) as f32);
        let root = [
            0.0,
            (0.95 * air - 0.018 * air * air).max(0.0),
            axis.map_or_else(
                || -12.0 + (0.9 * air).min(32.0),
                |axis| {
                    (if world.frame == 0 {
                        -12.0
                    } else {
                        world.view.root[2]
                    }) + axis * 0.9
                },
            ),
        ];
        if action == 1 && air > 0.0 && root[1] == 0.0 {
            let fighter = world.movement.as_mut().expect("canonical Pigeon State");
            fighter.action.id = 0;
            fighter.action.frame = 0;
        }
        root
    };
    let fighter = world.movement.as_ref().expect("canonical Pigeon State");
    let action = fighter.action.id;
    let frame = fighter.action.frame.min(actions[action].frames.len() - 1);
    let source = &actions[action].frames[frame];
    let mut view = Tick {
        action,
        frame,
        root,
        damage: fighter.combat.percent,
        contact: false,
        hit: None,
    };
    for hb in &source.hit_boxes {
        let values = hb;
        if !values.enabled || !values.aerial {
            continue;
        }
        let p = hb.position;
        let target = world
            .bag
            .as_ref()
            .map_or([0.0, 24.0, 28.0], |bag| bag.position);
        let overlap = query::intersection_test(
            &Pose::translation(
                world.movement.as_ref().map_or(1.0, |s| s.facing) * p[0],
                p[1] + root[1] + source.y_pos,
                world.movement.as_ref().map_or(1.0, |s| s.facing) * (p[2] + source.x_pos) + root[2],
            ),
            &Ball::new(hb.radius),
            &Pose::translation(target[0], target[1], target[2]),
            &Cuboid::new(Vec3::new(3.0, 6.0, 4.0)),
        )
        .unwrap();
        view.contact |= overlap;
        if overlap && !world.attack_hit {
            let grounded = world.bag.as_ref().is_some_and(|bag| bag.grounded);
            let percent = world.movement.as_ref().expect("canonical Pigeon State").combat.percent;
            let outcome = contact_outcome(values, percent, grounded);
            if let Some(bag) = &mut world.bag {
                bag.launch(&outcome);
            }
            world
                .movement
                .as_mut()
                .expect("canonical Pigeon State")
                .combat
                .percent = outcome.percent_after;
            world.attack_hit = true;
            world.hit_count += 1;
            world.last_hit = Some(world.frame);
            view.hit = Some((hb.id, values.damage));
        }
    }
    view.damage = world.movement.as_ref().expect("canonical Pigeon State").combat.percent;
    world.view = view;
    world.frame += 1;
    let fighter = world.movement.as_mut().expect("canonical Pigeon State");
    fighter.action.frame += 1;
    if !imported && fighter.action.id == 2 && fighter.action.frame == actions[2].frames.len() {
        fighter.action.id = 0;
        fighter.action.frame = 0;
    }
    if fighter.action.id == 0 {
        fighter.action.frame %= actions[0].frames.len();
    }
}

/// A durable copy of simulation state. Asset data is shared separately.
pub struct Snapshot(World);
pub struct Simulation {
    actions: Arc<[Action]>,
    world: World,
}
impl Simulation {
    pub fn new(actions: Arc<[Action]>, launch: bool) -> Self {
        assert!(matches!(actions.len(), 3 | 7) || actions.len() == catalog::ACTION_COUNT);
        assert!(actions.iter().all(|a| !a.frames.is_empty()));
        let mut world = World::default();
        world.movement = Some(game_fighter::State::new(&movement::rules()));
        if launch {
            world.bag = Some(sandbag::Sandbag::default());
        }
        Self { actions, world }
    }
    pub fn new_locomotion(actions: Arc<[Action]>, launch: bool) -> Self {
        assert_eq!(actions.len(), catalog::ACTION_COUNT);
        let mut simulation = Self::new(actions, launch);
        simulation.world.movement = Some(movement::initial());
        simulation
    }
    pub fn advance(&mut self, input: u8) -> &World {
        advance_world(&mut self.world, input, &self.actions);
        &self.world
    }
    pub fn state(&self) -> &World {
        &self.world
    }
    /// Horizontal position lives in snapshot-owned World.
    /// Vertical travel retains the lab's existing jump curve.
    #[tracing::instrument(target = "pigeon::simulation", level = "trace", skip_all, fields(tick = self.world.frame, input, axis))]
    pub fn advance_controlled(&mut self, input: u8, axis: f32) -> &World {
        assert!(axis.is_finite() && (-1.0..=1.0).contains(&axis));
        advance_with_axis(&mut self.world, input, &self.actions, Some(axis));
        &self.world
    }
    #[tracing::instrument(target = "pigeon::snapshot", level = "trace", skip_all, fields(tick = self.world.frame))]
    pub fn save(&self) -> Snapshot {
        Snapshot(self.world.clone())
    }
    #[tracing::instrument(target = "pigeon::snapshot", level = "trace", skip_all, fields(tick = snapshot.0.frame))]
    pub fn load(&mut self, snapshot: &Snapshot) {
        self.world = snapshot.0.clone();
    }
}

#[cfg(test)]
#[path = "2_tests.rs"]
mod tests;
