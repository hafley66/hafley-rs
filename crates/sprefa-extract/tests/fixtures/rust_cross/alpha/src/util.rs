use crate::shapes::perimeter;
use crate::{shapes::{area, Circle}, VERSION_TAG};

pub const LABEL: &str = VERSION_TAG;

pub fn unit_area() -> f64 {
    let circle = Circle::new(1.0);
    area(&circle) + crate::shapes::area(&circle) + circle.radius + perimeter(1.0)
}
