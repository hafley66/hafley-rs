//! Swept contact resolution. Parry supplies authoritative contact normals and Rapier's kinematic
//! controller supplies the corrected compound-shape translation. Authored one-sided admission is
//! decided before the upstream query scene is built; no rigid-body dynamics run here.

use crate::admits_unilateral_contact;
use glam::Vec2;
use rapier2d::control::{CharacterLength, KinematicCharacterController};
use rapier2d::parry::{query, shape::SharedShape};
use rapier2d::prelude::{
    ActiveCollisionTypes, BroadPhaseBvh, ColliderBuilder, ColliderSet, CollisionPipeline,
    IslandManager, NarrowPhase, Pose, QueryFilter, RigidBodySet, Vector,
};

/// One stationary solid face set: an axis-aligned extent a moving body may clamp against.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SolidSurface {
    pub center: Vec2,
    pub half: Vec2,
    /// One-sided: only catches bodies crossing its top while travelling into the normal, and only
    /// when `admits_unilateral_contact` admits the candidate.
    pub one_way: bool,
}

pub(crate) const CONTACT_EPSILON: f32 = 0.001;
const CONTACT_RATIO: f32 = 0.005;

/// Upstream contact normal most aligned with `up` over the complete compound shape. One-sided
/// solids are admitted before the query and expose only their upper segment.
pub fn contact_from_surfaces(
    parts: &[(Vec2, Vec2)],
    solids: &[SolidSurface],
    up: Vec2,
    intent: Option<Vec2>,
) -> Option<Vec2> {
    if parts.is_empty() {
        return None;
    }
    let compound = SharedShape::compound(
        parts
            .iter()
            .map(|(center, half)| {
                (
                    Pose::translation(center.x, center.y),
                    SharedShape::cuboid(half.x, half.y),
                )
            })
            .collect(),
    );
    let prediction = parts.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(min, max), (center, half)| (min.min(center.y - half.y), max.max(center.y + half.y)),
    );
    let prediction = ((prediction.1 - prediction.0) * CONTACT_RATIO).max(CONTACT_EPSILON * 2.0);
    let query_prediction = prediction + CONTACT_EPSILON;
    let mut best: Option<Vec2> = None;
    let mut best_alignment = f32::NEG_INFINITY;
    for solid in solids {
        if solid.one_way && !admits_unilateral_contact(up, Vec2::ZERO, intent) {
            continue;
        }
        let top = solid.center.y - solid.half.y;
        let solid_shape = if solid.one_way {
            SharedShape::cuboid(solid.half.x, CONTACT_EPSILON)
        } else {
            SharedShape::cuboid(solid.half.x, solid.half.y)
        };
        let solid_position = if solid.one_way {
            Pose::translation(solid.center.x, top + CONTACT_EPSILON)
        } else {
            Pose::translation(solid.center.x, solid.center.y)
        };
        let Ok(Some(contact)) = query::contact(
            &Pose::IDENTITY,
            compound.as_ref(),
            &solid_position,
            solid_shape.as_ref(),
            query_prediction,
        ) else {
            continue;
        };
        if contact.dist.abs() > query_prediction {
            continue;
        }
        let normal = -Vec2::new(contact.normal1.x, contact.normal1.y);
        if solid.one_way && normal.dot(Vec2::NEG_Y) <= 0.0 {
            continue;
        }
        let alignment = normal.dot(up);
        if alignment > best_alignment {
            best_alignment = alignment;
            best = Some(normal);
        }
    }
    best
}

/// Rapier KCC solve for a compound shape. Its corrected translation and upstream collision normals
/// are authoritative; authored one-sided admission only controls which colliders enter the query.
pub fn sweep_against_surfaces(
    start: Vec2,
    velocity: Vec2,
    parts: &[(Vec2, Vec2)],
    solids: &[SolidSurface],
    contact_normal: Vec2,
    intent: Option<Vec2>,
) -> (Vec2, Vec2) {
    if parts.is_empty() || velocity == Vec2::ZERO {
        return (start + velocity, velocity);
    }
    let (character_shape, character_position) = if parts.len() == 1 {
        let (center, half) = parts[0];
        (
            SharedShape::cuboid(half.x, half.y),
            Pose::translation(center.x, center.y),
        )
    } else {
        (
            SharedShape::compound(
                parts
                    .iter()
                    .map(|(center, half)| {
                        (
                            Pose::translation(center.x - start.x, center.y - start.y),
                            SharedShape::cuboid(half.x, half.y),
                        )
                    })
                    .collect(),
            ),
            Pose::translation(start.x, start.y),
        )
    };
    let desired = contact_from_surfaces(parts, solids, contact_normal, intent)
        .filter(|normal| velocity.dot(*normal) < 0.0)
        .map_or(velocity, |normal| {
            crate::ContactBasis::new(normal).tangent_projection(velocity)
        });
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    for solid in solids {
        let admitted = !solid.one_way
            || (admits_unilateral_contact(contact_normal, velocity, intent)
                && parts.iter().any(|(center, half)| {
                    center.y + half.y <= solid.center.y - solid.half.y + CONTACT_EPSILON
                }));
        if !admitted {
            continue;
        }
        if contact_from_surfaces(parts, std::slice::from_ref(solid), contact_normal, intent)
            .is_some_and(|normal| desired.dot(normal) >= 0.0)
        {
            continue;
        }
        let builder = if solid.one_way {
            ColliderBuilder::segment(
                Vector::new(-solid.half.x, -solid.half.y),
                Vector::new(solid.half.x, -solid.half.y),
            )
            .translation(Vector::new(solid.center.x, solid.center.y))
        } else {
            ColliderBuilder::cuboid(solid.half.x, solid.half.y)
                .translation(Vector::new(solid.center.x, solid.center.y))
        };
        colliders.insert(
            builder
                .active_collision_types(ActiveCollisionTypes::all())
                .build(),
        );
    }
    if colliders.is_empty() {
        return (start + desired, desired);
    }
    let mut broad_phase = BroadPhaseBvh::new();
    let mut narrow_phase = NarrowPhase::new();
    CollisionPipeline::new().step(
        0.0,
        &mut IslandManager::new(),
        &mut broad_phase,
        &mut narrow_phase,
        &mut bodies,
        &mut colliders,
        &(),
        &(),
    );
    let queries = broad_phase.as_query_pipeline(
        narrow_phase.query_dispatcher(),
        &bodies,
        &colliders,
        QueryFilter::default(),
    );
    let controller = KinematicCharacterController {
        up: Vector::new(contact_normal.x, contact_normal.y),
        offset: CharacterLength::Absolute(CONTACT_EPSILON),
        autostep: None,
        snap_to_ground: None,
        normal_nudge_factor: 0.0,
        ..KinematicCharacterController::default()
    };
    let movement = controller.move_shape(
        1.0 / 60.0,
        &queries,
        character_shape.as_ref(),
        &character_position,
        Vector::new(desired.x, desired.y),
        |_| {},
    );
    let corrected = Vec2::new(movement.translation.x, movement.translation.y);
    let velocity = if movement.grounded {
        crate::ContactBasis::new(contact_normal).tangent_projection(desired)
    } else {
        desired
    };
    (start + corrected, velocity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(center: Vec2, half: Vec2) -> SolidSurface {
        SolidSurface {
            center,
            half,
            one_way: false,
        }
    }

    // The detector reports the touched face whose normal best opposes `up`, with no world axis
    // baked in: the same body-vs-box abutment grounds on a floor when up is up and on a wall when
    // up points sideways. This is what lets a body stand under a translating radial gravity field.
    #[test]
    fn contact_from_surfaces_reports_the_face_opposing_up() {
        let block = [solid(Vec2::ZERO, Vec2::splat(10.0))]; // faces at x=+-10, y=+-10
        let half = Vec2::splat(1.0);

        // Resting on the block's top (part underside at y=-10): up-is-up grounds on that face.
        let on_top = [(Vec2::new(0.0, -11.0), half)];
        assert_eq!(
            contact_from_surfaces(&on_top, &block, Vec2::new(0.0, -1.0), None),
            Some(Vec2::new(0.0, -1.0)),
        );

        // Pressed against the block's right face (part left at x=10): sideways gravity (up points
        // right) grounds on the WALL — the same detector, no code change per orientation.
        let on_right = [(Vec2::new(11.0, 0.0), half)];
        assert_eq!(
            contact_from_surfaces(&on_right, &block, Vec2::new(1.0, 0.0), None),
            Some(Vec2::new(1.0, 0.0)),
        );

        // Touching nothing: no contact regardless of up.
        let away = [(Vec2::new(50.0, 50.0), half)];
        assert_eq!(
            contact_from_surfaces(&away, &block, Vec2::new(0.0, -1.0), None),
            None,
        );
    }
}
