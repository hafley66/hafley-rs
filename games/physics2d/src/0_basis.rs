use glam::Vec2;

/// Orthonormal 2D basis derived per node per tick from a live vector and discarded. Basis
/// coordinates exist only between a `decompose` and its `recompose`; nothing resolved against a
/// basis is ever persisted (docs/physics-units-spec.md).
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Basis2 {
    pub along: Vec2,
    pub across: Vec2,
}

impl Basis2 {
    /// Basis whose `along` axis is `v` normalized; `fallback` (assumed unit) is used when `v`
    /// is degenerate (zero or non-finite). `across` is `along` rotated a quarter turn.
    pub fn from_vector(v: Vec2, fallback: Vec2) -> Self {
        let length_sq = v.length_squared();
        let along = if length_sq.is_finite() && length_sq > 0.0 {
            v / length_sq.sqrt()
        } else {
            fallback
        };
        Self {
            along,
            across: along.perp(),
        }
    }

    /// World vector -> basis coordinates `(v . along, v . across)`.
    pub fn decompose(self, v: Vec2) -> Vec2 {
        Vec2::new(v.dot(self.along), v.dot(self.across))
    }

    /// Basis coordinates -> world vector.
    pub fn recompose(self, c: Vec2) -> Vec2 {
        self.along * c.x + self.across * c.y
    }

    /// Mirror across the `along` axis: negate the `across` coordinate.
    pub fn reflect(self, v: Vec2) -> Vec2 {
        let c = self.decompose(v);
        self.recompose(Vec2::new(c.x, -c.y))
    }
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
    fn from_vector_normalizes_and_falls_back_when_degenerate() {
        let basis = Basis2::from_vector(Vec2::new(0.0, 5.0), Vec2::X);
        assert_vec2_close(basis.along, Vec2::Y);
        assert!((basis.along.dot(basis.across)).abs() < EPS);
        let degenerate = Basis2::from_vector(Vec2::ZERO, Vec2::X);
        assert_vec2_close(degenerate.along, Vec2::X);
        assert_vec2_close(degenerate.across, Vec2::Y);
    }
    #[test]
    fn decompose_recompose_round_trips_and_reflect_negates_across() {
        let basis = Basis2::from_vector(Vec2::new(3.0, -4.0), Vec2::X);
        let v = Vec2::new(4.0, 3.0);
        assert_vec2_close(basis.recompose(basis.decompose(v)), v);
        let reflected = basis.reflect(v);
        assert_vec2_close(basis.reflect(reflected), v);
        assert!((basis.decompose(reflected).x - basis.decompose(v).x).abs() < EPS);
        assert!((basis.decompose(reflected).y + basis.decompose(v).y).abs() < EPS);
    }
}
