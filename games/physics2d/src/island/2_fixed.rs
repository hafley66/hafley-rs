use glam::Vec2;
use rapier2d::prelude::{
    ActiveHooks, CoefficientCombineRule, ColliderBuilder, Pose, RigidBodyBuilder, RigidBodyType,
    SharedShape,
};

use super::{SolverIsland, directional_contacts::ONE_WAY_COLLIDER};

impl SolverIsland {
    /// Upsert one fixed body whose single compound collider is assembled from world-space part
    /// centers and half extents. Compound child poses are relative to the body's `position`.
    pub fn drive_fixed(
        &mut self,
        key: u64,
        position: Vec2,
        restitution: f32,
        parts: &[(Vec2, Vec2)],
    ) {
        assert!(
            !parts.is_empty(),
            "a fixed compound requires at least one part"
        );
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
        self.drive_fixed_shape(key, position, compound, false, restitution);
    }

    fn drive_fixed_shape(
        &mut self,
        key: u64,
        position: Vec2,
        shape: SharedShape,
        one_way: bool,
        restitution: f32,
    ) {
        let handle = self.upsert_fixed_body(key, position);
        self.retain_body_collider_count(handle, 1);
        let collider = self
            .bodies
            .get(handle)
            .and_then(|body| body.colliders().first().copied());
        if let Some(collider) = collider {
            if let Some(collider) = self.colliders.get_mut(collider) {
                collider.set_shape(shape);
                collider.set_position_wrt_parent(Pose::IDENTITY);
                collider.set_restitution(restitution);
                collider.set_restitution_combine_rule(CoefficientCombineRule::Max);
                collider.user_data = u128::from(one_way) * ONE_WAY_COLLIDER;
                collider.set_active_hooks(active_hooks(one_way));
            }
        } else {
            let builder = ColliderBuilder::new(shape)
                .restitution(restitution)
                .restitution_combine_rule(CoefficientCombineRule::Max)
                .user_data(u128::from(one_way) * ONE_WAY_COLLIDER)
                .active_hooks(active_hooks(one_way));
            self.colliders
                .insert_with_parent(builder.build(), handle, &mut self.bodies);
        }
    }

    /// Upsert one fixed body whose collider is the exact ordered line strip supplied by the
    /// caller-facing adapter. Rapier owns the polyline narrow phase and contact generation.
    pub fn drive_fixed_polyline(
        &mut self,
        key: u64,
        position: Vec2,
        vertices: &[Vec2],
        one_way: bool,
        restitution: f32,
    ) {
        assert!(
            vertices.len() >= 2,
            "a fixed polyline requires at least two vertices"
        );
        let handle = self.upsert_fixed_body(key, position);
        let existing = self
            .bodies
            .get(handle)
            .map(|body| body.colliders().to_vec())
            .unwrap_or_default();
        for (part, segment) in vertices.windows(2).enumerate() {
            let shape = SharedShape::polyline(
                segment
                    .iter()
                    .map(|vertex| {
                        rapier2d::math::Vector::new(vertex.x - position.x, vertex.y - position.y)
                    })
                    .collect(),
                None,
            );
            if let Some(&collider) = existing.get(part) {
                if let Some(collider) = self.colliders.get_mut(collider) {
                    collider.set_shape(shape);
                    collider.set_position_wrt_parent(Pose::IDENTITY);
                    collider.set_restitution(restitution);
                    collider.set_restitution_combine_rule(CoefficientCombineRule::Max);
                    collider.user_data = u128::from(one_way) * ONE_WAY_COLLIDER;
                    collider.set_active_hooks(active_hooks(one_way));
                }
            } else {
                self.colliders.insert_with_parent(
                    ColliderBuilder::new(shape)
                        .restitution(restitution)
                        .restitution_combine_rule(CoefficientCombineRule::Max)
                        .user_data(u128::from(one_way) * ONE_WAY_COLLIDER)
                        .active_hooks(active_hooks(one_way))
                        .build(),
                    handle,
                    &mut self.bodies,
                );
            }
        }
        self.retain_body_collider_count(handle, vertices.len() - 1);
    }

    fn upsert_fixed_body(
        &mut self,
        key: u64,
        position: Vec2,
    ) -> rapier2d::prelude::RigidBodyHandle {
        let target = rapier2d::math::Vector::new(position.x, position.y);
        let handle = if let Some(&(_, handle)) = self.tracked.iter().find(|(id, _)| *id == key) {
            handle
        } else {
            let handle = self
                .bodies
                .insert(RigidBodyBuilder::fixed().translation(target).build());
            self.tracked.push((key, handle));
            handle
        };
        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_body_type(RigidBodyType::Fixed, true);
            body.set_translation(target, true);
        }
        handle
    }
}

fn active_hooks(_one_way: bool) -> ActiveHooks {
    ActiveHooks::MODIFY_SOLVER_CONTACTS
}
