use super::*;

pub fn helper<T: Debug>(value: T) -> usize {
    format!("{value:?}").len()
}
