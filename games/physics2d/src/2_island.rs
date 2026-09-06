//! Solver-owned dynamic island sharing a fixed tick with caller-controlled bodies.
//!
//! Rollback contract: the island rolls back by `Clone` inside the ggrs state cell, the
//! same clone-snapshot form used by the calling simulation. Warmstart
//! impulses live in the cloned `NarrowPhase`, so restored re-simulations warmstart
//! identically. `PhysicsPipeline` is scratch-only by upstream design and is rebuilt per
//! step; `CCDSolver` is a ZST.

use glam::Vec2;
use rapier2d::prelude::{
    ActiveHooks, CCDSolver, CoefficientCombineRule, ColliderBuilder, ColliderSet, ImpulseJointSet,
    IntegrationParameters, IslandManager, MultibodyJointSet, NarrowPhase, PhysicsPipeline, Pose,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, RigidBodyType, SharedShape,
};
use serde::{Deserialize, Serialize};

#[path = "island/1_contacts.rs"]
mod contacts;
#[path = "island/3_controlled.rs"]
mod controlled;
#[path = "island/0_directional_contacts.rs"]
mod directional_contacts;
#[path = "island/2_fixed.rs"]
mod fixed;

pub use contacts::{MAX_PHYSICS_CONTACTS, PhysicsContact};
pub use controlled::{ColliderPartKey, ControlledHit, ControlledMovement};
use directional_contacts::DirectionalContactHooks;

/// Global gravity is zero by construction: field forces are per-body, applied by the
/// caller before `step` (the radial-well form the experiment validated). Tracked bodies
/// are fiat-driven kinematic position-based bodies — the one-way seam: the solver never
/// moves them, dynamic bodies react to them through contacts.
#[derive(Clone, Serialize, Deserialize)]
pub struct SolverIsland {
    params: IntegrationParameters,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    islands: IslandManager,
    broad_phase: rapier2d::prelude::BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    ccd: CCDSolver,
    /// Caller-keyed tracked bodies in caller insertion order (order is state; it rolls
    /// back with the clone).
    tracked: Vec<(u64, RigidBodyHandle)>,
    /// Replaced after every Rapier step and copied into the caller's rollback state.
    contacts: Vec<PhysicsContact>,
}

impl Default for SolverIsland {
    fn default() -> Self {
        Self {
            // rapier defaults: dt = 1/60, matching the tick natively.
            params: IntegrationParameters::default(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            islands: IslandManager::new(),
            broad_phase: rapier2d::prelude::BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            ccd: CCDSolver::new(),
            tracked: Vec::new(),
            contacts: Vec::new(),
        }
    }
}

impl SolverIsland {
    /// Canonical persisted solver state. Rapier's maintained serde surface retains every
    /// continuation-relevant set and phase; upstream-marked workspaces rebuild on decode.
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("SolverIsland serialization")
    }

    /// Restore a solver state captured by [`Self::snapshot_bytes`].
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    /// Rapier's integration interval in seconds. Its velocity setters consume per-second values.
    pub fn time_step(&self) -> f32 {
        self.params.dt
    }

    fn retain_body_collider_count(&mut self, handle: RigidBodyHandle, count: usize) {
        let excess = self
            .bodies
            .get(handle)
            .map(|body| {
                body.colliders()
                    .iter()
                    .copied()
                    .skip(count)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for collider in excess {
            self.colliders
                .remove(collider, &mut self.islands, &mut self.bodies, true);
        }
    }

    /// Upsert one solver-moved body whose single compound collider is assembled from world-space
    /// part centers and half extents. Rapier owns its response after the commanded pose and velocity.
    #[allow(clippy::too_many_arguments)]
    pub fn drive_dynamic(
        &mut self,
        key: u64,
        position: Vec2,
        velocity: Vec2,
        rotation: f32,
        angular_velocity: f32,
        rotation_locked: bool,
        density: f32,
        angular_damping: f32,
        // vector-note: Rapier defines soft CCD look-ahead as one isotropic scalar distance.
        soft_ccd_prediction: f32,
        restitution: f32,
        parts: &[(Vec2, Vec2)],
    ) {
        assert!(
            !parts.is_empty(),
            "a dynamic compound requires at least one part"
        );
        let target = rapier2d::math::Vector::new(position.x, position.y);
        let linear_velocity = rapier2d::math::Vector::new(velocity.x, velocity.y);
        let compound = SharedShape::compound(
            parts
                .iter()
                .map(|(center, half)| {
                    (
                        Pose::translation(center.x - position.x, center.y - position.y),
                        SharedShape::cuboid(half.x, half.y),
                    )
                })
                .collect(),
        );
        let handle = if let Some(&(_, handle)) = self.tracked.iter().find(|(id, _)| *id == key) {
            handle
        } else {
            let handle = self.bodies.insert(
                RigidBodyBuilder::dynamic()
                    .soft_ccd_prediction(soft_ccd_prediction)
                    .build(),
            );
            self.tracked.push((key, handle));
            handle
        };
        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_body_type(RigidBodyType::Dynamic, true);
            body.set_translation(target, true);
            body.set_linvel(linear_velocity, true);
            body.set_rotation(rapier2d::math::Rotation::new(rotation), true);
            body.set_angvel(angular_velocity, true);
            body.lock_rotations(rotation_locked, true);
            body.set_angular_damping(angular_damping);
            body.set_soft_ccd_prediction(soft_ccd_prediction);
        }
        self.retain_body_collider_count(handle, 1);
        let collider = self
            .bodies
            .get(handle)
            .and_then(|body| body.colliders().first().copied());
        if let Some(collider) = collider {
            if let Some(collider) = self.colliders.get_mut(collider) {
                collider.set_shape(compound);
                collider.set_position_wrt_parent(Pose::IDENTITY);
                collider.set_density(density);
                collider.set_restitution(restitution);
                collider.set_restitution_combine_rule(CoefficientCombineRule::Max);
                collider.user_data = 0;
                collider.set_active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS);
            }
        } else {
            self.colliders.insert_with_parent(
                ColliderBuilder::new(compound)
                    .density(density)
                    .restitution(restitution)
                    .restitution_combine_rule(CoefficientCombineRule::Max)
                    .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
                    .build(),
                handle,
                &mut self.bodies,
            );
        }
    }

    /// Read solver-owned motion for one caller key.
    pub fn tracked_motion(&self, key: u64) -> Option<(Vec2, Vec2, f32, f32)> {
        let (_, handle) = self.tracked.iter().find(|(id, _)| *id == key)?;
        let body = self.bodies.get(*handle)?;
        let position = body.translation();
        let velocity = body.linvel();
        Some((
            Vec2::new(position.x, position.y),
            Vec2::new(velocity.x, velocity.y),
            body.rotation().angle(),
            body.angvel(),
        ))
    }

    /// Upsert the kinematic position-based body for `key` (cuboid `half`) and command its
    /// pose for the coming step. `set_next_kinematic_translation` is the exact-fiat seam:
    /// the solver derives contact velocity from the commanded pose but never writes back.
    pub fn drive_tracked(&mut self, key: u64, half: Vec2, position: Vec2, restitution: f32) {
        let target = rapier2d::math::Vector::new(position.x, position.y);
        let handle = match self.tracked.iter().find(|(id, _)| *id == key) {
            Some(&(_, handle)) => handle,
            None => {
                let handle = self.bodies.insert(
                    RigidBodyBuilder::kinematic_position_based()
                        .translation(target)
                        .build(),
                );
                self.tracked.push((key, handle));
                handle
            }
        };
        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_body_type(RigidBodyType::KinematicPositionBased, true);
            body.set_translation(target, true);
            body.set_rotation(rapier2d::math::Rotation::identity(), true);
            body.set_next_kinematic_translation(target);
        }
        self.retain_body_collider_count(handle, 1);
        let collider = self
            .bodies
            .get(handle)
            .and_then(|body| body.colliders().first().copied());
        if let Some(collider) = collider {
            if let Some(collider) = self.colliders.get_mut(collider) {
                collider.set_shape(SharedShape::cuboid(half.x, half.y));
                collider.set_position_wrt_parent(Pose::IDENTITY);
                collider.set_restitution(restitution);
                collider.set_restitution_combine_rule(CoefficientCombineRule::Max);
                collider.user_data = 0;
                collider.set_active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS);
            }
        } else {
            self.colliders.insert_with_parent(
                ColliderBuilder::cuboid(half.x, half.y)
                    .restitution(restitution)
                    .restitution_combine_rule(CoefficientCombineRule::Max)
                    .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
                    .build(),
                handle,
                &mut self.bodies,
            );
        }
    }

    /// Drop tracked bodies whose key is absent from this tick's set (their colliders and
    /// any joints go with them).
    pub fn retain_tracked(&mut self, keys: &[u64]) {
        let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.tracked)
            .into_iter()
            .partition(|(key, _)| keys.contains(key));
        self.tracked = kept;
        for (_, handle) in dropped {
            self.bodies.remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
    }

    /// One solver tick. The pipeline itself is scratch and rebuilt per call.
    pub fn step(&mut self) {
        let hooks = DirectionalContactHooks::new();
        PhysicsPipeline::new().step(
            rapier2d::math::Vector::new(0.0, 0.0),
            &self.params,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            &hooks,
            &(),
        );
        self.publish_contacts(hooks.take_pre_solver_contacts());
    }

    /// Dynamic (solver-moved) bodies only; tracked fiat bodies are excluded.
    pub fn dynamic_body_count(&self) -> usize {
        self.bodies
            .iter()
            .filter(|(_, body)| body.is_dynamic())
            .count()
    }
}
