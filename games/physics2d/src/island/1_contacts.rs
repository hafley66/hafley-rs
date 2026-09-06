use glam::Vec2;
use rapier2d::prelude::{ColliderHandle, RigidBodyHandle};
use serde::{Deserialize, Serialize};

use super::{SolverIsland, directional_contacts::PreSolverContact};

/// A content-neutral fact copied from Rapier's post-step narrow phase. `a < b` is
/// canonical and `normal_a_to_b` is oriented with that identity order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsContact {
    pub a: u64,
    pub b: u64,
    pub a_part: u32,
    pub b_part: u32,
    pub normal_a_to_b: Vec2,
    pub point: Vec2,
    pub pre_solver_relative_normal_velocity: f32,
    pub relative_normal_velocity: f32,
    pub restitution: f32,
    pub is_new: bool,
}

/// Fixed rollback capacity for published solver contacts. This is deliberately a
/// presentation/fact budget, never a cap on Rapier's collision processing.
pub const MAX_PHYSICS_CONTACTS: usize = 128;

impl SolverIsland {
    /// Post-step facts from Rapier's maintained narrow-phase manifolds. This observes
    /// solver contacts only; it neither detects nor resolves any collision itself.
    pub fn contacts(&self) -> &[PhysicsContact] {
        &self.contacts
    }

    pub(super) fn publish_contacts(&mut self, mut pre_solver_contacts: Vec<PreSolverContact>) {
        let mut facts = Vec::new();
        for pair in self.narrow_phase.contact_pairs() {
            let Some(collider1) = self.colliders.get(pair.collider1) else {
                continue;
            };
            let Some(collider2) = self.colliders.get(pair.collider2) else {
                continue;
            };
            let Some(body1) = collider1.parent() else {
                continue;
            };
            let Some(body2) = collider2.parent() else {
                continue;
            };
            let Some(key1) = self.key_for_body(body1) else {
                continue;
            };
            let Some(key2) = self.key_for_body(body2) else {
                continue;
            };
            let Some(rigid1) = self.bodies.get(body1) else {
                continue;
            };
            let Some(rigid2) = self.bodies.get(body2) else {
                continue;
            };
            let part1 = self.part_for_collider(body1, pair.collider1);
            let part2 = self.part_for_collider(body2, pair.collider2);
            for manifold in &pair.manifolds {
                let normal = Vec2::new(manifold.data.normal.x, manifold.data.normal.y);
                for contact in &manifold.data.solver_contacts {
                    let point = Vec2::new(contact.point.x, contact.point.y);
                    let velocity1 = rigid1.velocity_at_point(contact.point);
                    let velocity2 = rigid2.velocity_at_point(contact.point);
                    let relative =
                        Vec2::new(velocity2.x - velocity1.x, velocity2.y - velocity1.y).dot(normal);
                    let (a, b, a_part, b_part, normal_a_to_b, relative_normal_velocity) =
                        if key1 <= key2 {
                            (key1, key2, part1, part2, normal, relative)
                        } else {
                            (key2, key1, part2, part1, -normal, -relative)
                        };
                    let pre_solver_relative_normal_velocity = pre_solver_contacts
                        .iter()
                        .position(|pre_solver| {
                            pre_solver.collider1 == pair.collider1
                                && pre_solver.collider2 == pair.collider2
                                && pre_solver.contact_id == contact.contact_id[0]
                        })
                        .map(|index| pre_solver_contacts.remove(index).relative_normal_velocity)
                        .unwrap_or_default();
                    let pre_solver_relative_normal_velocity = if key1 <= key2 {
                        pre_solver_relative_normal_velocity
                    } else {
                        -pre_solver_relative_normal_velocity
                    };
                    facts.push(PhysicsContact {
                        a,
                        b,
                        a_part,
                        b_part,
                        normal_a_to_b,
                        point,
                        pre_solver_relative_normal_velocity,
                        relative_normal_velocity,
                        restitution: contact.restitution,
                        is_new: contact.is_new != 0.0,
                    });
                }
            }
        }
        facts.sort_by_key(physics_contact_key);
        facts.truncate(MAX_PHYSICS_CONTACTS);
        self.contacts = facts;
    }

    fn key_for_body(&self, body: RigidBodyHandle) -> Option<u64> {
        self.tracked
            .iter()
            .find_map(|(key, handle)| (*handle == body).then_some(*key))
    }

    fn part_for_collider(&self, body: RigidBodyHandle, collider: ColliderHandle) -> u32 {
        self.bodies
            .get(body)
            .and_then(|body| {
                body.colliders()
                    .iter()
                    .position(|handle| *handle == collider)
            })
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(u32::MAX)
    }
}

fn physics_contact_key(contact: &PhysicsContact) -> (u64, u64, u32, u32, u32, u32, u32) {
    (
        contact.a,
        contact.b,
        contact.a_part,
        contact.b_part,
        contact.point.x.to_bits(),
        contact.point.y.to_bits(),
        contact.normal_a_to_b.x.to_bits() ^ contact.normal_a_to_b.y.to_bits(),
    )
}
