use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DurableWorld {
    pub tick: u32,
    pub physics: PhysicsWorld,
}

impl DurableWorld {
    pub fn new() -> Self {
        let mut physics = PhysicsWorld::default();
        physics.integration_parameters.dt = 1.0 / 60.0;
        physics.insert(RigidBodyBuilder::fixed().translation(Vector::new(0.0, -0.25, 0.0)), ColliderBuilder::cuboid(8.0, 0.25, 8.0));
        physics.insert(RigidBodyBuilder::dynamic().translation(Vector::new(-1.0, 3.0, 0.0)).linvel(Vector::new(1.25, 0.0, 0.0)), ColliderBuilder::capsule_y(0.45, 0.2).restitution(0.2));
        Self { tick: 0, physics }
    }

    pub fn advance(&mut self) {
        self.physics.step();
        self.tick += 1;
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
}

impl Default for DurableWorld {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapier_full_durable_state_restores_and_physics_replays_exactly() {
        let mut world = DurableWorld::new();
        for _ in 0..37 { world.advance(); }
        let snapshot = world.canonical_bytes();
        for _ in 0..81 { world.advance(); }
        let expected = world.canonical_bytes();
        let mut restored: DurableWorld = serde_json::from_slice(&snapshot).unwrap();
        for _ in 0..81 { restored.advance(); }
        assert_eq!(restored.canonical_bytes(), expected);
    }

    #[test]
    fn snapshot_does_not_alias_current_world() {
        let mut world = DurableWorld::new();
        let snapshot = world.canonical_bytes();
        world.advance();
        assert_eq!(serde_json::from_slice::<DurableWorld>(&snapshot).unwrap().tick, 0);
        assert_ne!(snapshot, world.canonical_bytes());
    }
}
