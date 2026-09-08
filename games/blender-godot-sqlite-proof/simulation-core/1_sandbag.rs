use crate::Attack;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use ssbm_utils::{calc, enums::character::Attributes};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Hovering,
    Hit,
    Hitstun,
    Falling,
    Landed,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hovering => "HOVERING",
            Self::Hit => "HIT",
            Self::Hitstun => "HITSTUN",
            Self::Falling => "FALLING",
            Self::Landed => "LANDED",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Sandbag {
    physics: PhysicsWorld,
    body: RigidBodyHandle,
    pub phase: Phase,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub stun: u32,
    pub knockback: f32,
    pub grounded: bool,
}

// Full Rapier durable state is copied, including solver/contact state.
// This deliberately allocation-heavy snapshot fixture is not a hot-buffer benchmark.
impl Clone for Sandbag {
    fn clone(&self) -> Self {
        serde_json::from_slice(&serde_json::to_vec(self).unwrap()).unwrap()
    }
}
impl PartialEq for Sandbag {
    fn eq(&self, other: &Self) -> bool {
        serde_json::to_vec(self).unwrap() == serde_json::to_vec(other).unwrap()
    }
}
impl std::fmt::Debug for Sandbag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbag")
            .field("phase", &self.phase)
            .field("position", &self.position)
            .field("velocity", &self.velocity)
            .field("stun", &self.stun)
            .finish()
    }
}

impl Default for Sandbag {
    fn default() -> Self {
        let mut physics = PhysicsWorld::default();
        physics.integration_parameters.dt = 1.0 / 60.0;
        // PM coordinates are retained as lab units. Acceleration converts units/tick² to units/s².
        physics.gravity = Vector::new(0.0, -0.095 * 3600.0, 0.0);
        physics.insert(
            RigidBodyBuilder::fixed().translation(Vector::new(0.0, -1.0, 50.0)),
            ColliderBuilder::cuboid(20.0, 1.0, 180.0).friction(1.0),
        );
        let (body, _) = physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(0.0, 24.0, 28.0))
                .gravity_scale(0.0)
                .lock_rotations()
                .linear_damping(0.8),
            ColliderBuilder::cuboid(3.0, 6.0, 4.0)
                .restitution(0.0)
                .friction(1.0),
        );
        Self {
            physics,
            body,
            phase: Phase::Hovering,
            position: [0.0, 24.0, 28.0],
            velocity: [0.0; 3],
            stun: 0,
            knockback: 0.0,
            grounded: false,
        }
    }
}

impl Sandbag {
    pub fn launch(&mut self, hit: &Attack, percent_before: f32) {
        let target = Attributes::MARIO; // Only name/weight are read by this knockback helper; weight=100.
        self.knockback = calc::knockback(
            hit.damage,
            hit.damage,
            hit.kbg,
            hit.bkb,
            hit.wdsk,
            false,
            &target,
            percent_before,
            false,
            false,
            false,
            false,
            false,
            false,
        );
        let angle = calc::resolve_sakurai_angle(hit.trajectory.to_radians(), self.knockback, false);
        self.velocity = [
            0.0,
            calc::initial_y_velocity(self.knockback, angle, false),
            calc::initial_x_velocity(self.knockback, angle),
        ];
        self.stun = calc::hitstun(self.knockback);
        let body = self.physics.bodies.get_mut(self.body).unwrap();
        body.set_gravity_scale(1.0, true);
        body.set_linvel(Vector::from(self.velocity) * 60.0, true);
        self.phase = Phase::Hit;
        self.grounded = false;
    }

    pub fn advance(&mut self) {
        if self.phase == Phase::Hovering {
            return;
        }
        self.physics.step();
        let body = &self.physics.bodies[self.body];
        self.position = body.translation().to_array();
        self.velocity = (body.linvel() / 60.0).to_array();
        self.grounded = self
            .physics
            .narrow_phase
            .contact_pairs()
            .any(|pair| pair.has_any_active_contact());
        self.stun = self.stun.saturating_sub(1);
        self.phase = if self.grounded {
            Phase::Landed
        } else if self.stun > 0 {
            Phase::Hitstun
        } else {
            Phase::Falling
        };
    }
}
