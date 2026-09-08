use std::sync::Arc;
#[path = "0_types.rs"]
mod types;
pub use types::*;
#[path = "1a_actions.rs"]
mod lifecycle;
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub frame: i32,
    pub action: usize,
    pub animation: usize,
    pub jump_at: Option<i32>,
    pub previous_input: u8,
    pub attack_hit: bool,
    pub damage: f32,
    pub hit_count: usize,
    pub last_hit: Option<i32>,
    pub view: Tick,
    pub bag: Option<sandbag::Sandbag>,
}

pub fn fixture_input(tick: i32) -> u8 {
    match tick {
        60 => JUMP,
        78 => ATTACK,
        _ => 0,
    }
}

#[tracing::instrument(target = "falcon::simulation", level = "trace", skip_all, fields(tick = world.frame, input = bits))]
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

    fn reduce(world: &mut World, (bits, axis): Self::Event, actions: Self::Context<'_>, _: &mut impl FnMut(Self::Effect)) {
        reduce_tick(world, bits, actions, axis);
    }
}

fn reduce_tick(world: &mut World, bits: u8, actions: &[Action], axis: Option<f32>) {
    if let Some(bag) = &mut world.bag {
        bag.advance();
    }
    let pressed = bits & !world.previous_input;
    let imported = actions.len() == 7;
    let root = if imported {
        lifecycle::advance(world, pressed, actions, axis.unwrap_or(0.0))
    } else {
        if pressed & JUMP != 0 && world.view.root[1] == 0.0 {
            world.jump_at = Some(world.frame);
            world.action = 1;
            world.animation = 0;
        }
        if pressed & ATTACK != 0 && world.jump_at.is_some() && world.view.root[1] > 0.0 {
            world.action = 2;
            world.animation = 0;
            world.attack_hit = false;
        }
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
        if world.action == 1 && air > 0.0 && root[1] == 0.0 {
            world.action = 0;
            world.animation = 0;
        }
        root
    };
    let frame = world.animation.min(actions[world.action].frames.len() - 1);
    let source = &actions[world.action].frames[frame];
    let mut view = Tick {
        action: world.action,
        frame,
        root,
        damage: world.damage,
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
                p[0],
                p[1] + root[1] + source.y_pos,
                p[2] + root[2] + source.x_pos,
            ),
            &Ball::new(hb.radius),
            &Pose::translation(target[0], target[1], target[2]),
            &Cuboid::new(Vec3::new(3.0, 6.0, 4.0)),
        )
        .unwrap();
        view.contact |= overlap;
        if overlap && !world.attack_hit {
            if let Some(bag) = &mut world.bag {
                bag.launch(values, world.damage);
            }
            world.damage += values.damage;
            world.attack_hit = true;
            world.hit_count += 1;
            world.last_hit = Some(world.frame);
            view.hit = Some((hb.id, values.damage));
        }
    }
    view.damage = world.damage;
    world.view = view;
    world.previous_input = bits;
    world.frame += 1;
    world.animation += 1;
    if !imported && world.action == 2 && world.animation == actions[2].frames.len() {
        world.action = 0;
        world.animation = 0;
    }
    if world.action == 0 {
        world.animation %= actions[0].frames.len();
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
        assert!(matches!(actions.len(), 3 | 7));
        assert!(actions.iter().all(|a| !a.frames.is_empty()));
        let mut world = World::default();
        if launch {
            world.bag = Some(sandbag::Sandbag::default());
        }
        Self { actions, world }
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
    #[tracing::instrument(target = "falcon::simulation", level = "trace", skip_all, fields(tick = self.world.frame, input, axis))]
    pub fn advance_controlled(&mut self, input: u8, axis: f32) -> &World {
        assert!(axis.is_finite() && (-1.0..=1.0).contains(&axis));
        advance_with_axis(&mut self.world, input, &self.actions, Some(axis));
        &self.world
    }
    #[tracing::instrument(target = "falcon::snapshot", level = "trace", skip_all, fields(tick = self.world.frame))]
    pub fn save(&self) -> Snapshot {
        Snapshot(self.world.clone())
    }
    #[tracing::instrument(target = "falcon::snapshot", level = "trace", skip_all, fields(tick = snapshot.0.frame))]
    pub fn load(&mut self, snapshot: &Snapshot) {
        self.world = snapshot.0.clone();
    }
}
