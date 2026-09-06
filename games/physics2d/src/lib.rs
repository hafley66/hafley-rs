//! Deterministic 2D integration and Rapier-backed solver state.
//!
//! The crate accepts vectors, scalar coefficients, shapes, and stable caller keys. It has no
//! authored-content, application-world, input-device, or presentation dependencies.

use glam::Vec2;

#[path = "0_basis.rs"]
mod basis;
#[path = "2_island.rs"]
mod island;
#[path = "1_sweep.rs"]
mod sweep;
pub use basis::Basis2;
pub use island::{
    ColliderPartKey, ControlledHit, ControlledMovement, MAX_PHYSICS_CONTACTS, PhysicsContact,
    SolverIsland,
};
pub use sweep::{SolidSurface, contact_from_surfaces, sweep_against_surfaces};

/// Resolved per node per tick. `friction` and `max_velocity` are active-basis coordinates:
/// contact basis when grounded (medium velocity = the surface's), gravity basis
/// otherwise (medium velocity zero). Coord 0 is `along`, coord 1 is `across`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct EffectivePhysics {
    pub gravity: Vec2,
    /// Add-per-frame, world vector.
    pub force: Vec2,
    /// Linear counter-accel toward the medium velocity, approach-with-clamp (never multiplicative).
    pub friction: Vec2,
    /// Along cap is one-sided (min, from above); across cap symmetric.
    pub max_velocity: Vec2,
}

/// Contact basis for a grounded body: `basis.along` is the contact normal, `basis.across` the
/// surface tangent. `surface_velocity` is the medium the body's velocity is measured against.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ContactBasis {
    pub basis: Basis2,
    pub surface_velocity: Vec2,
}

impl ContactBasis {
    pub fn new(normal: Vec2) -> Self {
        Self::with_surface_velocity(normal, Vec2::ZERO)
    }

    pub fn with_surface_velocity(normal: Vec2, surface_velocity: Vec2) -> Self {
        Self {
            basis: Basis2::from_vector(normal, -Vec2::Y),
            surface_velocity,
        }
    }

    pub fn normal(self) -> Vec2 {
        self.basis.along
    }

    pub fn tangent_projection(self, velocity: Vec2) -> Vec2 {
        self.basis.across * velocity.dot(self.basis.across)
    }
}

/// One integration step: add gravity and force, then resolve friction and caps in the active
/// basis relative to the medium velocity. Airborne the active basis derives from the gravity
/// vector (world axes when gravity is degenerate) and the medium is at rest.
pub fn integrate_velocity(
    velocity: Vec2,
    values: EffectivePhysics,
    contact: Option<ContactBasis>,
) -> Vec2 {
    let velocity = velocity + values.gravity + values.force;
    // dl-guard: contact-vs-airborne active-basis selection; every friction/max_velocity clamp resolves against this
    let (basis, medium) = match contact {
        Some(contact) => (contact.basis, contact.surface_velocity),
        None => (Basis2::from_vector(values.gravity, Vec2::Y), Vec2::ZERO),
    };
    let mut coords = basis.decompose(velocity - medium);
    coords.x = approach(coords.x, 0.0, values.friction.x);
    coords.y = approach(coords.y, 0.0, values.friction.y);
    coords.x = coords.x.min(values.max_velocity.x);
    coords.y = coords
        .y
        .clamp(-values.max_velocity.y, values.max_velocity.y);
    medium + basis.recompose(coords)
}

/// Caller intent units used for unilateral-contact admission.
pub const UNILATERAL_INTENT_THRESHOLD: f32 = 50.0;

/// Contact admission for a one-sided face, decided before any contact exists. A candidate is
/// admitted only while the body approaches from the face's open side (relative velocity into
/// the normal) and the controller's stick intent does not command travel through the face.
/// Rejecting a candidate means no clamp is ever applied, not even for one frame, and because
/// admission is recomputed from current vectors every step, releasing the intent restores
/// contact on the next crossing with no stored per-body state.
pub fn admits_unilateral_contact(
    normal: Vec2,
    relative_velocity: Vec2,
    intent: Option<Vec2>,
) -> bool {
    relative_velocity.dot(normal) <= 0.0
        && intent.is_none_or(|intent| intent.dot(normal) > -UNILATERAL_INTENT_THRESHOLD)
}

/// Rate-limited scalar approach toward `target`, clamped so it cannot overshoot in one step.
pub fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}

/// Rotate `velocity` toward `target_direction` by at most `max_angle` radians (whichever
/// rotation direction is shorter), preserving `velocity`'s magnitude exactly. A zero `velocity`
/// (nothing to reorient) or a degenerate/non-finite `target_direction` (nothing to aim at)
/// returns `velocity` unchanged. The turn itself never calls `atan2`/`sin`/`cos` directly: the
/// signed angle between the two directions comes from a dot/perp-dot pair (pure multiply-add),
/// and the one bounded-angle-to-vector conversion needed to build the clamped step goes through
/// `glam::Vec2::from_angle`, the crate's approved libm-backed trig entry point (see
/// `docs/CARVE-REPORT.md`'s transcendental-boundary note; `glam` is pinned with its `libm`
/// feature workspace-wide specifically so this stays bit-identical native vs wasm).
pub fn rotate_toward(velocity: Vec2, target_direction: Vec2, max_angle: f32) -> Vec2 {
    let speed = velocity.length();
    if speed <= 0.0 || !target_direction.is_finite() || target_direction == Vec2::ZERO {
        return velocity;
    }
    let current = velocity / speed;
    let target = target_direction.normalize();
    let cos_delta = current.dot(target);
    let sin_delta = current.x * target.y - current.y * target.x;
    let step = Vec2::from_angle(max_angle.abs());
    if cos_delta >= step.x {
        // Already within the allowed deviation: snap fully onto the target direction.
        return target * speed;
    }
    let rotation = if sin_delta >= 0.0 {
        step
    } else {
        Vec2::new(step.x, -step.y)
    };
    current.rotate(rotation) * speed
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 0.000001;

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).abs().max_element() < EPS,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn contact_basis_projects_velocity_for_multiple_normals() {
        let cases = [
            (
                Vec2::new(0.0, -1.0),
                Vec2::new(3.0, 4.0),
                Vec2::new(3.0, 0.0),
                Vec2::new(0.0, 4.0),
            ),
            (
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
            ),
            (
                Vec2::new(3.0, -4.0),
                Vec2::new(4.0, 3.0),
                Vec2::new(4.0, 3.0),
                Vec2::ZERO,
            ),
        ];

        for (normal, velocity, expected_tangent, expected_normal) in cases {
            let basis = ContactBasis::new(normal);
            assert!((basis.normal().dot(basis.basis.across)).abs() < EPS);
            let normal_projection = basis.normal() * velocity.dot(basis.normal());
            assert_vec2_close(basis.tangent_projection(velocity), expected_tangent);
            assert_vec2_close(normal_projection, expected_normal);
            assert_vec2_close(
                basis.tangent_projection(velocity) + normal_projection,
                velocity,
            );
        }
    }

    #[test]
    fn friction_is_linear_approach_and_caps_are_frame_local_one_sided() {
        // Grounded: friction removes a fixed amount along the tangent, then the across cap clamps.
        let grounded = EffectivePhysics {
            gravity: Vec2::new(0.0, 2.0),
            force: Vec2::new(1.0, 0.0),
            friction: Vec2::new(0.0, 0.75),
            max_velocity: Vec2::new(f32::INFINITY, 5.0),
        };
        let floor = ContactBasis::new(Vec2::new(0.0, -1.0));
        assert_vec2_close(
            integrate_velocity(Vec2::new(10.0, 0.0), grounded, Some(floor)),
            Vec2::new(5.0, 2.0),
        );

        // Airborne: the along (with-gravity) cap is one-sided; against-gravity is uncapped.
        let airborne = EffectivePhysics {
            gravity: Vec2::new(0.0, 1.0),
            force: Vec2::ZERO,
            friction: Vec2::ZERO,
            max_velocity: Vec2::new(3.0, f32::INFINITY),
        };
        assert_vec2_close(
            integrate_velocity(Vec2::new(0.0, 4.0), airborne, None),
            Vec2::new(0.0, 3.0),
        );
        assert_vec2_close(
            integrate_velocity(Vec2::new(0.0, -9.0), airborne, None),
            Vec2::new(0.0, -8.0),
        );
    }

    #[test]
    fn rotate_toward_preserves_magnitude_and_clamps_the_turn() {
        // Small target deviation, well inside the clamp: the turn goes all the way through.
        let turned = rotate_toward(Vec2::new(10.0, 0.0), Vec2::new(1.0, 1.0), 1.0);
        assert!((turned.length() - 10.0).abs() < EPS);
        assert!(turned.y > 0.0);

        // Target is a hard reversal; the clamp must cap the turn to `max_angle` this call.
        let clamped = rotate_toward(Vec2::new(10.0, 0.0), Vec2::new(-1.0, 0.0), 0.1);
        assert!((clamped.length() - 10.0).abs() < EPS);
        let angle_turned = Vec2::new(10.0, 0.0).angle_to(clamped).abs();
        assert!((angle_turned - 0.1).abs() < 0.0001);

        // Degenerate inputs are no-ops: nothing to rotate, or nothing to aim at.
        assert_vec2_close(
            rotate_toward(Vec2::ZERO, Vec2::new(1.0, 0.0), 1.0),
            Vec2::ZERO,
        );
        assert_vec2_close(
            rotate_toward(Vec2::new(3.0, 4.0), Vec2::ZERO, 1.0),
            Vec2::new(3.0, 4.0),
        );
    }

    #[test]
    fn contact_basis_friction_uses_relative_surface_velocity() {
        let values = EffectivePhysics {
            gravity: Vec2::ZERO,
            force: Vec2::ZERO,
            friction: Vec2::new(0.0, 0.5),
            max_velocity: Vec2::INFINITY,
        };
        let floor = ContactBasis::with_surface_velocity(Vec2::new(0.0, -1.0), Vec2::new(2.0, 1.0));
        assert_vec2_close(
            integrate_velocity(Vec2::new(6.0, 3.0), values, Some(floor)),
            Vec2::new(5.5, 3.0),
        );
    }
}
