use glam::Vec2;
use rapier2d::{
    control::{CharacterCollision, CharacterLength, KinematicCharacterController},
    prelude::{Collider, ColliderHandle, QueryFilter, RigidBodyHandle, Vector},
};

use super::{SolverIsland, directional_contacts::ONE_WAY_COLLIDER};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ColliderPartKey {
    pub body: u64,
    pub part: u32,
}

#[derive(Copy, Clone, Debug)]
pub struct ControlledHit {
    pub part: ColliderPartKey,
    pub translation_applied: Vec2,
    pub translation_remaining: Vec2,
    pub normal: Vec2,
    pub time_of_impact: f32,
}

#[derive(Clone, Debug)]
pub struct ControlledMovement {
    pub translation: Vec2,
    pub grounded: bool,
    pub sliding: bool,
    pub hits: Vec<ControlledHit>,
}

impl SolverIsland {
    /// Publish manually driven body poses to their colliders before deriving scene queries. The
    /// authoritative broad/narrow phase remains owned by the one real physics step; KCC derives a
    /// scratch BVH from these persistent sets so query preparation cannot start solver contacts.
    pub fn refresh_queries(&mut self) {
        self.bodies
            .propagate_modified_body_positions_to_colliders(&mut self.colliders);
    }

    /// Correct desired translation against the persistent Rapier scene and command the body's
    /// next position-kinematic pose. Admission filters one-way collider parts before KCC casts.
    pub fn move_controlled(
        &mut self,
        key: u64,
        desired_translation: Vec2,
        up: Vec2,
        admit: impl Fn(ColliderPartKey, Vec2) -> bool,
    ) -> Option<ControlledMovement> {
        let &(_, body_handle) = self.tracked.iter().find(|(body, _)| *body == key)?;
        let body = self.bodies.get(body_handle)?;
        let collider_handle = body.colliders().first().copied()?;
        let collider = self.colliders.get(collider_handle)?;
        let character_shape = collider.shared_shape().clone();
        let character_position = *collider.position();
        let current_body_translation = body.translation();

        let tracked = &self.tracked;
        let bodies = &self.bodies;
        let colliders = &self.colliders;
        let predicate = |handle: ColliderHandle, collider: &Collider| {
            if collider.user_data & ONE_WAY_COLLIDER == 0 {
                return true;
            }
            collider_part_key(tracked, bodies, collider, handle)
                .zip(segment_left_normal(collider))
                .is_some_and(|(part, normal)| admit(part, normal))
        };
        let filter = QueryFilter::default()
            .exclude_rigid_body(body_handle)
            .predicate(&predicate);
        let mut query_broad_phase = self.broad_phase.clone();
        let modified = colliders
            .iter()
            .map(|(handle, _)| handle)
            .collect::<Vec<_>>();
        let mut pair_events = Vec::new();
        query_broad_phase.update(
            &self.params,
            colliders,
            bodies,
            &modified,
            &[],
            &mut pair_events,
        );
        let queries = query_broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            bodies,
            colliders,
            filter,
        );
        let controller = KinematicCharacterController {
            up: Vector::new(up.x, up.y),
            offset: CharacterLength::Absolute(crate::sweep::CONTACT_EPSILON),
            slide: true,
            autostep: None,
            snap_to_ground: None,
            normal_nudge_factor: 0.0,
            ..KinematicCharacterController::default()
        };
        let mut hits = Vec::new();
        let movement = controller.move_shape(
            self.params.dt,
            &queries,
            character_shape.as_ref(),
            &character_position,
            Vector::new(desired_translation.x, desired_translation.y),
            |collision| {
                if let Some(hit) = controlled_hit(tracked, bodies, colliders, collision) {
                    hits.push(hit);
                }
            },
        );
        let corrected = Vec2::new(movement.translation.x, movement.translation.y);
        if let Some(body) = self.bodies.get_mut(body_handle) {
            body.set_next_kinematic_translation(
                current_body_translation + Vector::new(corrected.x, corrected.y),
            );
        }
        Some(ControlledMovement {
            translation: corrected,
            grounded: movement.grounded,
            sliding: movement.is_sliding_down_slope,
            hits,
        })
    }
}

fn controlled_hit(
    tracked: &[(u64, RigidBodyHandle)],
    bodies: &rapier2d::prelude::RigidBodySet,
    colliders: &rapier2d::prelude::ColliderSet,
    collision: CharacterCollision,
) -> Option<ControlledHit> {
    let collider = colliders.get(collision.handle)?;
    let part = collider_part_key(tracked, bodies, collider, collision.handle)?;
    Some(ControlledHit {
        part,
        translation_applied: Vec2::new(
            collision.translation_applied.x,
            collision.translation_applied.y,
        ),
        translation_remaining: Vec2::new(
            collision.translation_remaining.x,
            collision.translation_remaining.y,
        ),
        normal: Vec2::new(collision.hit.normal1.x, collision.hit.normal1.y),
        time_of_impact: collision.hit.time_of_impact,
    })
}

fn collider_part_key(
    tracked: &[(u64, RigidBodyHandle)],
    bodies: &rapier2d::prelude::RigidBodySet,
    collider: &Collider,
    collider_handle: ColliderHandle,
) -> Option<ColliderPartKey> {
    let parent = collider.parent()?;
    let &(body, _) = tracked.iter().find(|(_, handle)| *handle == parent)?;
    let part = bodies
        .get(parent)?
        .colliders()
        .iter()
        .position(|handle| *handle == collider_handle)?;
    Some(ColliderPartKey {
        body,
        part: part.try_into().ok()?,
    })
}

fn segment_left_normal(collider: &Collider) -> Option<Vec2> {
    let segment = collider.shape().as_polyline()?.segment(0);
    let local = -segment.normal()?;
    let world = collider.rotation() * local;
    Some(Vec2::new(world.x, world.y))
}
