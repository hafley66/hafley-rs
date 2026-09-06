use std::sync::{Arc, Mutex};

use rapier2d::prelude::{Collider, ColliderHandle, ContactModificationContext, PhysicsHooks};

pub(super) const ONE_WAY_COLLIDER: u128 = 1;

pub(super) struct DirectionalContactHooks {
    pre_solver_contacts: Arc<Mutex<Vec<PreSolverContact>>>,
}

#[derive(Clone, Copy)]
pub(super) struct PreSolverContact {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    pub contact_id: u32,
    pub relative_normal_velocity: f32,
}

impl DirectionalContactHooks {
    pub(super) fn new() -> Self {
        Self {
            pre_solver_contacts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn take_pre_solver_contacts(&self) -> Vec<PreSolverContact> {
        std::mem::take(
            &mut *self
                .pre_solver_contacts
                .lock()
                .expect("pre-solver contact observation lock"),
        )
    }
}

impl PhysicsHooks for DirectionalContactHooks {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext<'_>) {
        let collider1 = &context.colliders[context.collider1];
        let collider2 = &context.colliders[context.collider2];
        let one_way1 = collider1.user_data & ONE_WAY_COLLIDER != 0;
        let one_way2 = collider2.user_data & ONE_WAY_COLLIDER != 0;
        if let Some(allowed_local_n1) = if one_way1 {
            polyline_left_normal(collider1, context.manifold.subshape1)
        } else if one_way2 {
            polyline_left_normal(collider2, context.manifold.subshape2)
                .map(|normal| collider1.rotation().inverse() * -(collider2.rotation() * normal))
        } else {
            None
        } {
            context.update_as_oneway_platform(allowed_local_n1, std::f32::consts::FRAC_PI_2);
        }
        self.observe_pre_solver_contacts(context);
    }
}

impl DirectionalContactHooks {
    fn observe_pre_solver_contacts(&self, context: &ContactModificationContext<'_>) {
        let Some(body1) = context
            .rigid_body1
            .and_then(|handle| context.bodies.get(handle))
        else {
            return;
        };
        let Some(body2) = context
            .rigid_body2
            .and_then(|handle| context.bodies.get(handle))
        else {
            return;
        };
        let normal = *context.normal;
        let mut observations = self
            .pre_solver_contacts
            .lock()
            .expect("pre-solver contact observation lock");
        observations.extend(context.solver_contacts.iter().map(|contact| {
            PreSolverContact {
                collider1: context.collider1,
                collider2: context.collider2,
                contact_id: contact.contact_id[0],
                relative_normal_velocity: (body2.velocity_at_point(contact.point)
                    - body1.velocity_at_point(contact.point))
                .dot(normal),
            }
        }));
    }
}

fn polyline_left_normal(collider: &Collider, subshape: u32) -> Option<rapier2d::math::Vector> {
    let segment = collider.shape().as_polyline()?.segment(subshape);
    segment.normal().map(|normal| -normal)
}
