use beta::base::Unit;

pub struct Circle {
    pub(crate) radius: f64,
}

impl Circle {
    pub(crate) fn new(radius: f64) -> Self {
        Circle { radius }
    }
}

pub(crate) fn area(circle: &Circle) -> f64 {
    circle.radius * circle.radius * delta::scale() * Unit::one()
}

pub fn perimeter(radius: f64) -> f64 {
    radius * 6.0
}
