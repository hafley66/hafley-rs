//! Clean-room Rapier 0.34 comparison of discrete contact, soft CCD, and hard CCD against an
//! ordered one-way polyline. This is an evidence harness, not a second motion resolver: every
//! pose and velocity printed below is Rapier output. The curved rows cover both the peak and one
//! off-center facet. They establish contact admission only, not any content-specific landing or
//! containment policy.

use rapier2d::prelude::*;

const ONE_WAY: u128 = 1;

struct OneWay;

impl PhysicsHooks for OneWay {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext<'_>) {
        let collider1 = &context.colliders[context.collider1];
        let collider2 = &context.colliders[context.collider2];
        let Some(allowed_local_n1) = (if collider1.user_data == ONE_WAY {
            left_normal(collider1, context.manifold.subshape1)
        } else if collider2.user_data == ONE_WAY {
            left_normal(collider2, context.manifold.subshape2)
                .map(|normal| collider1.rotation().inverse() * -(collider2.rotation() * normal))
        } else {
            None
        }) else {
            return;
        };
        context.update_as_oneway_platform(allowed_local_n1, std::f32::consts::FRAC_PI_2);
    }
}

fn left_normal(collider: &Collider, subshape: u32) -> Option<Vector> {
    let segment = collider.shape().as_polyline()?.segment(subshape);
    let tangent = (segment.b - segment.a).try_normalize()?;
    Some(Vector::new(-tangent.y, tangent.x))
}

fn run(
    vertices: &[Vector],
    soft_prediction: Real,
    start: Vector,
    velocity: Vector,
    hard_ccd: bool,
) -> (Vector, Vector) {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let mut islands = IslandManager::new();
    let mut broad = BroadPhaseBvh::new();
    let mut narrow = NarrowPhase::new();
    let mut impulse_joints = ImpulseJointSet::new();
    let mut multibody_joints = MultibodyJointSet::new();
    let mut ccd = CCDSolver::new();

    let fixed = bodies.insert(RigidBodyBuilder::fixed().build());
    colliders.insert_with_parent(
        ColliderBuilder::polyline(vertices.to_vec(), None)
            .user_data(ONE_WAY)
            .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
            .build(),
        fixed,
        &mut bodies,
    );

    let probe = bodies.insert(
        RigidBodyBuilder::dynamic()
            .translation(start)
            .linvel(velocity)
            .ccd_enabled(hard_ccd)
            .soft_ccd_prediction(soft_prediction)
            .lock_rotations()
            .build(),
    );
    colliders.insert_with_parent(
        ColliderBuilder::cuboid(1.0, 1.0).build(),
        probe,
        &mut bodies,
    );

    PhysicsPipeline::new().step(
        Vector::new(0.0, 0.0),
        &IntegrationParameters::default(),
        &mut islands,
        &mut broad,
        &mut narrow,
        &mut bodies,
        &mut colliders,
        &mut impulse_joints,
        &mut multibody_joints,
        &mut ccd,
        &OneWay,
        &(),
    );

    (bodies[probe].translation(), bodies[probe].linvel())
}

fn main() {
    let flat = [Vector::new(12.0, -5.0), Vector::new(-12.0, -5.0)];
    let arc = [
        Vector::new(12.0, 0.0),
        Vector::new(8.0, -3.0),
        Vector::new(4.0, -4.7),
        Vector::new(0.0, -5.0),
        Vector::new(-4.0, -4.7),
        Vector::new(-8.0, -3.0),
        Vector::new(-12.0, 0.0),
    ];
    for (name, vertices, soft, start, velocity, hard) in [
        (
            "flat-discrete-down",
            flat.as_slice(),
            0.0,
            Vector::new(0.0, -9.0),
            Vector::new(0.0, 600.0),
            false,
        ),
        (
            "flat-soft-down",
            flat.as_slice(),
            12.0,
            Vector::new(0.0, -9.0),
            Vector::new(0.0, 600.0),
            false,
        ),
        (
            "flat-soft-up",
            flat.as_slice(),
            12.0,
            Vector::new(0.0, 1.0),
            Vector::new(0.0, -600.0),
            false,
        ),
        (
            "flat-hard-down",
            flat.as_slice(),
            0.0,
            Vector::new(0.0, -9.0),
            Vector::new(0.0, 600.0),
            true,
        ),
        (
            "flat-hard-up",
            flat.as_slice(),
            0.0,
            Vector::new(0.0, 1.0),
            Vector::new(0.0, -600.0),
            true,
        ),
        (
            "arc-soft-peak-down",
            arc.as_slice(),
            12.0,
            Vector::new(0.0, -9.0),
            Vector::new(0.0, 600.0),
            false,
        ),
        (
            "arc-soft-peak-up",
            arc.as_slice(),
            12.0,
            Vector::new(0.0, 1.0),
            Vector::new(0.0, -600.0),
            false,
        ),
        (
            "arc-soft-facet-down",
            arc.as_slice(),
            12.0,
            Vector::new(6.0, -9.0),
            Vector::new(0.0, 600.0),
            false,
        ),
        (
            "arc-soft-facet-up",
            arc.as_slice(),
            12.0,
            Vector::new(6.0, 1.0),
            Vector::new(0.0, -600.0),
            false,
        ),
    ] {
        let (position, velocity) = run(vertices, soft, start, velocity, hard);
        println!(
            "{name} x={:.6} y={:.6} vx={:.6} vy={:.6}",
            position.x, position.y, velocity.x, velocity.y
        );
    }
}
