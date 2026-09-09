//! Dynamic presentation metadata shared by decoded and offline-baked assets.
use smash::fighters::falcon;
use super::boundary::contracts::{FrameValues, TargetValues, Row};
use falcon::{World, sandbag::Phase};

pub fn state(world: &World, animation_x: f64, animation_y: f64, predicted: bool, applied: u8) -> [Row; 2] {
    let s = &world.view;
    let tick = i64::from(world.frame - 1);
    let meta = FrameValues {
        action: s.action as f64, pose: s.frame as f64,
        root_x: s.root[0] as f64, root_y: s.root[1] as f64, root_z: s.root[2] as f64,
        damage: world.damage as f64, hits: world.hit_count as f64,
        last_hit: world.last_hit.unwrap_or(-1) as f64,
        contact: f64::from(s.contact), predicted: f64::from(predicted), input: f64::from(applied),
        animation_x, animation_y, ..Default::default()
    }.into_row(tick, 0);
    let mut target = TargetValues::default();
    if let Some(b) = &world.bag {
        [target.x, target.y, target.z] = b.position.map(f64::from);
        [target.vx, target.vy, target.vz] = b.velocity.map(f64::from);
        target.stun = b.stun as f64;
        target.phase = match b.phase {
            Phase::Hovering => 0.0, Phase::Hit => 1.0, Phase::Hitstun => 2.0,
            Phase::Falling => 3.0, Phase::Landed => 4.0,
        };
        target.grounded = f64::from(b.grounded);
    } else {
        target.y = 24.0;
        target.z = 28.0;
    }
    [meta, target.into_row(tick, 0)]
}
